//! Source-backed, resumable correspondent of Comrak's table-row scanner.

use std::{ops::Range, sync::Arc};

use serde::{Deserialize, Serialize};

use crate::{CancellationToken, MAX_TABLE_CELLS, Poll, ScanReceipt, physical_content_end};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CellSummary {
    pub source: Range<usize>,
    pub content: Range<usize>,
    pub internal_offset: usize,
    pub had_escaped_pipe: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableRowSummary {
    pub cells: Vec<CellSummary>,
    pub delimiter_alignments: Option<Vec<u8>>,
}

impl TableRowSummary {
    pub fn accounted_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + self.cells.capacity() * std::mem::size_of::<CellSummary>()
            + self.delimiter_alignments.as_ref().map_or(0, Vec::capacity)
    }
}

/// One source-backed cell emitted by the production-shaped scanner. A local
/// delimiter alignment is provisional until the row completion says every
/// cell was a valid delimiter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TableRowStreamCell {
    pub cell: CellSummary,
    pub delimiter_alignment: Option<u8>,
}

/// Constant-size row completion. Cell descriptors have already been handed
/// to the caller one at a time and may live in a persistent sequence.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableRowStreamSummary {
    pub cells: u32,
    pub delimiter_row: bool,
}

/// Why a syntactically valid delimiter line failed to promote the preceding
/// logical Paragraph line into a GFM table header.
///
/// Every variant is a `table_visited` rejection. A delimiter line which is
/// not itself syntactically valid is instead reported as `NotCandidate`, so a
/// later physical line may still try to open a Table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableHeaderRejectReason {
    HeaderNotRow,
    ColumnCountMismatch,
    TooManyColumns,
}

/// Terminal result of the read-only first pass over one retained logical
/// header line and the current physical delimiter line.
///
/// `Binding` is supplied by the actor which owns the immutable source and
/// projection cursors. This scanner never constructs or clones it; it merely
/// moves the same authority into the terminal result.
#[derive(Debug)]
pub enum TableHeaderDisposition<Binding> {
    NotCandidate {
        binding: Binding,
    },
    Rejected {
        binding: Binding,
        reason: TableHeaderRejectReason,
    },
    Ready(ValidatedTableHeader<Binding>),
}

/// One cooperative first-pass result. The job performs no output mutation and
/// retains no cells or alignment vector.
#[derive(Debug)]
pub enum TableHeaderPassOnePoll<Binding> {
    Pending {
        inspected: usize,
    },
    Complete {
        value: TableHeaderDisposition<Binding>,
        inspected: usize,
    },
    Cancelled {
        inspected: usize,
    },
}

impl<Binding> TableHeaderPassOnePoll<Binding> {
    pub const fn inspected(&self) -> usize {
        match self {
            Self::Pending { inspected }
            | Self::Complete { inspected, .. }
            | Self::Cancelled { inspected } => *inspected,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TableHeaderPassOnePhase {
    Delimiter,
    Header { columns: u32 },
    Complete,
}

/// Validate-then-replay pass one for a possible GFM table header.
///
/// The immutable row roots are owned `Arc` leases, not caller-resupplied byte
/// slices. A same-length source swap between polls is therefore impossible.
/// The actor binding should additionally name the source/projection roots,
/// Paragraph owner and cut, syntax options, container context, and candidate
/// epoch; this generic scanner cannot forge any of those domains.
#[derive(Debug)]
pub struct TableHeaderPassOneJob<Binding> {
    binding: Option<Binding>,
    header: Option<Arc<[u8]>>,
    delimiter: Option<Arc<[u8]>>,
    header_scan: StreamingTableRowJob,
    delimiter_scan: StreamingTableRowJob,
    phase: TableHeaderPassOnePhase,
}

impl<Binding> TableHeaderPassOneJob<Binding> {
    pub fn new(binding: Binding, header: Arc<[u8]>, delimiter: Arc<[u8]>) -> Self {
        Self {
            binding: Some(binding),
            header_scan: StreamingTableRowJob::new(&header),
            delimiter_scan: StreamingTableRowJob::new(&delimiter),
            header: Some(header),
            delimiter: Some(delimiter),
            phase: TableHeaderPassOnePhase::Delimiter,
        }
    }

    pub fn poll(
        &mut self,
        fuel: usize,
        cancellation: &CancellationToken,
    ) -> TableHeaderPassOnePoll<Binding> {
        assert!(fuel > 0);
        assert_ne!(self.phase, TableHeaderPassOnePhase::Complete);
        if cancellation.is_cancelled() {
            return TableHeaderPassOnePoll::Cancelled { inspected: 0 };
        }
        match self.phase {
            TableHeaderPassOnePhase::Delimiter => {
                let delimiter = self
                    .delimiter
                    .as_deref()
                    .expect("active Table validation retains its delimiter root");
                match self.delimiter_scan.poll(delimiter, fuel, cancellation) {
                    TableRowStreamPoll::Pending { inspected }
                    | TableRowStreamPoll::Cell { inspected, .. } => {
                        TableHeaderPassOnePoll::Pending { inspected }
                    }
                    TableRowStreamPoll::Cancelled { inspected } => {
                        TableHeaderPassOnePoll::Cancelled { inspected }
                    }
                    TableRowStreamPoll::Complete { value, inspected } => {
                        let Some(summary) = value.filter(|summary| summary.delimiter_row) else {
                            let binding = self.take_binding();
                            return self.complete(
                                TableHeaderDisposition::NotCandidate { binding },
                                inspected,
                            );
                        };
                        if usize::try_from(summary.cells)
                            .map_or(true, |cells| cells > MAX_TABLE_CELLS)
                        {
                            let binding = self.take_binding();
                            return self.complete(
                                TableHeaderDisposition::Rejected {
                                    binding,
                                    reason: TableHeaderRejectReason::TooManyColumns,
                                },
                                inspected,
                            );
                        }
                        self.phase = TableHeaderPassOnePhase::Header {
                            columns: summary.cells,
                        };
                        TableHeaderPassOnePoll::Pending { inspected }
                    }
                }
            }
            TableHeaderPassOnePhase::Header { columns } => {
                let header = self
                    .header
                    .as_deref()
                    .expect("active Table validation retains its header root");
                match self.header_scan.poll(header, fuel, cancellation) {
                    TableRowStreamPoll::Pending { inspected }
                    | TableRowStreamPoll::Cell { inspected, .. } => {
                        TableHeaderPassOnePoll::Pending { inspected }
                    }
                    TableRowStreamPoll::Cancelled { inspected } => {
                        TableHeaderPassOnePoll::Cancelled { inspected }
                    }
                    TableRowStreamPoll::Complete { value, inspected } => {
                        let Some(summary) = value else {
                            let binding = self.take_binding();
                            return self.complete(
                                TableHeaderDisposition::Rejected {
                                    binding,
                                    reason: TableHeaderRejectReason::HeaderNotRow,
                                },
                                inspected,
                            );
                        };
                        if summary.cells != columns {
                            let binding = self.take_binding();
                            return self.complete(
                                TableHeaderDisposition::Rejected {
                                    binding,
                                    reason: TableHeaderRejectReason::ColumnCountMismatch,
                                },
                                inspected,
                            );
                        }
                        let ready = ValidatedTableHeader {
                            binding: self.take_binding(),
                            header: self
                                .header
                                .take()
                                .expect("validated Table retains its header root"),
                            delimiter: self
                                .delimiter
                                .take()
                                .expect("validated Table retains its delimiter root"),
                            columns,
                        };
                        self.complete(TableHeaderDisposition::Ready(ready), inspected)
                    }
                }
            }
            TableHeaderPassOnePhase::Complete => {
                unreachable!("completed Table validation was rejected above")
            }
        }
    }

    fn take_binding(&mut self) -> Binding {
        self.binding
            .take()
            .expect("Table validation binding is consumed exactly once")
    }

    fn complete(
        &mut self,
        value: TableHeaderDisposition<Binding>,
        inspected: usize,
    ) -> TableHeaderPassOnePoll<Binding> {
        self.phase = TableHeaderPassOnePhase::Complete;
        TableHeaderPassOnePoll::Complete { value, inspected }
    }
}

/// Non-cloneable pass-two authority. It can only be minted by a successful
/// first pass and owns the exact same immutable header and delimiter roots.
#[must_use = "a validated Table header must be replayed or discarded with its candidate"]
#[derive(Debug)]
pub struct ValidatedTableHeader<Binding> {
    binding: Binding,
    header: Arc<[u8]>,
    delimiter: Arc<[u8]>,
    columns: u32,
}

impl<Binding> ValidatedTableHeader<Binding> {
    pub const fn binding(&self) -> &Binding {
        &self.binding
    }

    pub const fn columns(&self) -> u32 {
        self.columns
    }

    pub fn into_replay(self) -> TableHeaderReplayJob<Binding> {
        TableHeaderReplayJob {
            binding: Some(self.binding),
            header_scan: StreamingTableRowJob::new(&self.header),
            delimiter_scan: StreamingTableRowJob::new(&self.delimiter),
            header: self.header,
            delimiter: self.delimiter,
            columns: self.columns,
            next_column: 0,
            header_cell: None,
            delimiter_cell: None,
            header_complete: None,
            delimiter_complete: None,
            complete: false,
        }
    }
}

/// One pass-two paired cell. The header descriptor and exact delimiter cell
/// coverage came from fresh scanners over the roots certified by pass one.
#[must_use = "a replayed Table cell must enter the writer-owned transaction"]
#[derive(Debug, PartialEq, Eq)]
pub struct TableHeaderReplayCell {
    column: u32,
    header: CellSummary,
    delimiter: CellSummary,
    alignment: u8,
}

impl TableHeaderReplayCell {
    pub const fn column(&self) -> u32 {
        self.column
    }

    pub const fn header(&self) -> &CellSummary {
        &self.header
    }

    pub const fn delimiter(&self) -> &CellSummary {
        &self.delimiter
    }

    pub const fn alignment(&self) -> u8 {
        self.alignment
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableReplayError {
    CertifiedRootsChanged,
    ColumnCountChanged,
    DelimiterAlignmentMissing,
}

#[derive(Debug)]
pub enum TableHeaderReplayPoll<Binding> {
    Pending {
        inspected: usize,
    },
    Cell {
        value: TableHeaderReplayCell,
        inspected: usize,
    },
    Complete {
        binding: Binding,
        inspected: usize,
    },
    Cancelled {
        inspected: usize,
    },
    Failed {
        error: TableReplayError,
        inspected: usize,
    },
}

impl<Binding> TableHeaderReplayPoll<Binding> {
    pub const fn inspected(&self) -> usize {
        match self {
            Self::Pending { inspected }
            | Self::Cell { inspected, .. }
            | Self::Complete { inspected, .. }
            | Self::Cancelled { inspected }
            | Self::Failed { inspected, .. } => *inspected,
        }
    }
}

/// Fresh pass-two scanners over a pass-one-certified immutable header pair.
/// This type is intentionally non-serializable: an interrupted pass two is
/// restarted from the pre-work composite checkpoint and a new validation.
#[derive(Debug)]
pub struct TableHeaderReplayJob<Binding> {
    binding: Option<Binding>,
    header: Arc<[u8]>,
    delimiter: Arc<[u8]>,
    header_scan: StreamingTableRowJob,
    delimiter_scan: StreamingTableRowJob,
    columns: u32,
    next_column: u32,
    header_cell: Option<TableRowStreamCell>,
    delimiter_cell: Option<TableRowStreamCell>,
    header_complete: Option<Option<TableRowStreamSummary>>,
    delimiter_complete: Option<Option<TableRowStreamSummary>>,
    complete: bool,
}

impl<Binding> TableHeaderReplayJob<Binding> {
    pub const fn binding(&self) -> &Binding {
        self.binding
            .as_ref()
            .expect("active Table replay retains its actor binding")
    }

    pub const fn columns(&self) -> u32 {
        self.columns
    }

    pub fn poll(
        &mut self,
        fuel: usize,
        cancellation: &CancellationToken,
    ) -> TableHeaderReplayPoll<Binding> {
        assert!(fuel > 0);
        assert!(
            !self.complete,
            "Table replay must not be polled after Complete"
        );
        if cancellation.is_cancelled() {
            return TableHeaderReplayPoll::Cancelled { inspected: 0 };
        }
        if self.header_cell.is_some() && self.delimiter_cell.is_some() {
            return self.take_cell(0);
        }
        if self.header_complete.is_some() && self.delimiter_complete.is_some() {
            return self.finish(0);
        }

        if self.header_cell.is_none() && self.header_complete.is_none() {
            match self.header_scan.poll(&self.header, fuel, cancellation) {
                TableRowStreamPoll::Pending { inspected } => {
                    return TableHeaderReplayPoll::Pending { inspected };
                }
                TableRowStreamPoll::Cell { value, inspected } => {
                    self.header_cell = Some(value);
                    if self.delimiter_cell.is_some() {
                        return self.take_cell(inspected);
                    }
                    return TableHeaderReplayPoll::Pending { inspected };
                }
                TableRowStreamPoll::Complete { value, inspected } => {
                    self.header_complete = Some(value);
                    if self.delimiter_complete.is_some() {
                        return self.finish(inspected);
                    }
                    return TableHeaderReplayPoll::Pending { inspected };
                }
                TableRowStreamPoll::Cancelled { inspected } => {
                    return TableHeaderReplayPoll::Cancelled { inspected };
                }
            }
        }

        match self
            .delimiter_scan
            .poll(&self.delimiter, fuel, cancellation)
        {
            TableRowStreamPoll::Pending { inspected } => {
                TableHeaderReplayPoll::Pending { inspected }
            }
            TableRowStreamPoll::Cell { value, inspected } => {
                self.delimiter_cell = Some(value);
                if self.header_cell.is_some() {
                    self.take_cell(inspected)
                } else {
                    TableHeaderReplayPoll::Pending { inspected }
                }
            }
            TableRowStreamPoll::Complete { value, inspected } => {
                self.delimiter_complete = Some(value);
                if self.header_complete.is_some() {
                    self.finish(inspected)
                } else {
                    TableHeaderReplayPoll::Pending { inspected }
                }
            }
            TableRowStreamPoll::Cancelled { inspected } => {
                TableHeaderReplayPoll::Cancelled { inspected }
            }
        }
    }

    fn take_cell(&mut self, inspected: usize) -> TableHeaderReplayPoll<Binding> {
        let header = self
            .header_cell
            .take()
            .expect("paired Table replay has one header cell");
        let delimiter = self
            .delimiter_cell
            .take()
            .expect("paired Table replay has one delimiter cell");
        if self.next_column >= self.columns {
            return TableHeaderReplayPoll::Failed {
                error: TableReplayError::ColumnCountChanged,
                inspected,
            };
        }
        let Some(alignment) = delimiter.delimiter_alignment else {
            return TableHeaderReplayPoll::Failed {
                error: TableReplayError::DelimiterAlignmentMissing,
                inspected,
            };
        };
        let column = self.next_column;
        self.next_column += 1;
        TableHeaderReplayPoll::Cell {
            value: TableHeaderReplayCell {
                column,
                header: header.cell,
                delimiter: delimiter.cell,
                alignment,
            },
            inspected,
        }
    }

    fn finish(&mut self, inspected: usize) -> TableHeaderReplayPoll<Binding> {
        let header = self
            .header_complete
            .take()
            .expect("paired Table replay completed its header scanner");
        let delimiter = self
            .delimiter_complete
            .take()
            .expect("paired Table replay completed its delimiter scanner");
        let roots_match = matches!(
            (header, delimiter),
            (Some(header), Some(delimiter))
                if header.cells == self.columns
                    && delimiter.cells == self.columns
                    && delimiter.delimiter_row
                    && self.next_column == self.columns
                    && self.header_cell.is_none()
                    && self.delimiter_cell.is_none()
        );
        if !roots_match {
            return TableHeaderReplayPoll::Failed {
                error: TableReplayError::CertifiedRootsChanged,
                inspected,
            };
        }
        self.complete = true;
        TableHeaderReplayPoll::Complete {
            binding: self
                .binding
                .take()
                .expect("Table replay binding is returned exactly once"),
            inspected,
        }
    }
}

#[derive(Debug)]
pub enum TableBodyDisposition<Binding> {
    NotRow {
        binding: Binding,
    },
    Rejected {
        binding: Binding,
        reason: TableBodyRejectReason,
    },
    Ready(ValidatedTableBodyRow<Binding>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableBodyRejectReason {
    TooManyCells,
}

#[derive(Debug)]
pub enum TableBodyPassOnePoll<Binding> {
    Pending {
        inspected: usize,
    },
    Complete {
        value: TableBodyDisposition<Binding>,
        inspected: usize,
    },
    Cancelled {
        inspected: usize,
    },
}

/// Read-only first pass for a body row. The table width is already certified
/// by its header; pass two emits at most that many source cells and reports
/// exact padding/ignored counts at completion.
#[derive(Debug)]
pub struct TableBodyPassOneJob<Binding> {
    binding: Option<Binding>,
    row: Option<Arc<[u8]>>,
    scan: StreamingTableRowJob,
    columns: u32,
    complete: bool,
}

impl<Binding> TableBodyPassOneJob<Binding> {
    pub fn new(binding: Binding, row: Arc<[u8]>, columns: u32) -> Self {
        assert!(columns > 0);
        Self {
            binding: Some(binding),
            scan: StreamingTableRowJob::new(&row),
            row: Some(row),
            columns,
            complete: false,
        }
    }

    pub fn poll(
        &mut self,
        fuel: usize,
        cancellation: &CancellationToken,
    ) -> TableBodyPassOnePoll<Binding> {
        assert!(fuel > 0);
        assert!(
            !self.complete,
            "body validation must not be polled after Complete"
        );
        if cancellation.is_cancelled() {
            return TableBodyPassOnePoll::Cancelled { inspected: 0 };
        }
        let row = self
            .row
            .as_deref()
            .expect("active body validation retains its row root");
        match self.scan.poll(row, fuel, cancellation) {
            TableRowStreamPoll::Pending { inspected }
            | TableRowStreamPoll::Cell { inspected, .. } => {
                TableBodyPassOnePoll::Pending { inspected }
            }
            TableRowStreamPoll::Cancelled { inspected } => {
                TableBodyPassOnePoll::Cancelled { inspected }
            }
            TableRowStreamPoll::Complete { value, inspected } => {
                self.complete = true;
                let binding = self
                    .binding
                    .take()
                    .expect("body validation binding is consumed exactly once");
                let value = match value {
                    None => TableBodyDisposition::NotRow { binding },
                    Some(summary)
                        if usize::try_from(summary.cells)
                            .map_or(true, |cells| cells > MAX_TABLE_CELLS) =>
                    {
                        TableBodyDisposition::Rejected {
                            binding,
                            reason: TableBodyRejectReason::TooManyCells,
                        }
                    }
                    Some(summary) => TableBodyDisposition::Ready(ValidatedTableBodyRow {
                        binding,
                        row: self
                            .row
                            .take()
                            .expect("validated body row retains its immutable root"),
                        columns: self.columns,
                        source_cells: summary.cells,
                    }),
                };
                TableBodyPassOnePoll::Complete { value, inspected }
            }
        }
    }
}

#[must_use = "a validated Table body row must be replayed or discarded"]
#[derive(Debug)]
pub struct ValidatedTableBodyRow<Binding> {
    binding: Binding,
    row: Arc<[u8]>,
    columns: u32,
    source_cells: u32,
}

impl<Binding> ValidatedTableBodyRow<Binding> {
    pub const fn binding(&self) -> &Binding {
        &self.binding
    }

    pub const fn columns(&self) -> u32 {
        self.columns
    }

    pub const fn source_cells(&self) -> u32 {
        self.source_cells
    }

    pub fn into_replay(self) -> TableBodyReplayJob<Binding> {
        TableBodyReplayJob {
            binding: Some(self.binding),
            scan: StreamingTableRowJob::new(&self.row),
            row: self.row,
            columns: self.columns,
            expected_source_cells: self.source_cells,
            emitted: 0,
            complete: false,
        }
    }
}

#[must_use = "a replayed body cell must enter the writer-owned Table row"]
#[derive(Debug, PartialEq, Eq)]
pub struct TableBodyReplayCell {
    column: u32,
    cell: CellSummary,
}

impl TableBodyReplayCell {
    pub const fn column(&self) -> u32 {
        self.column
    }

    pub const fn cell(&self) -> &CellSummary {
        &self.cell
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TableBodyReplaySummary {
    pub source_cells: u32,
    pub emitted_cells: u32,
    pub padded_cells: u32,
    pub ignored_cells: u32,
}

#[derive(Debug)]
pub enum TableBodyReplayPoll<Binding> {
    Pending {
        inspected: usize,
    },
    Cell {
        value: TableBodyReplayCell,
        inspected: usize,
    },
    Complete {
        binding: Binding,
        value: TableBodyReplaySummary,
        inspected: usize,
    },
    Cancelled {
        inspected: usize,
    },
    Failed {
        error: TableReplayError,
        inspected: usize,
    },
}

#[derive(Debug)]
pub struct TableBodyReplayJob<Binding> {
    binding: Option<Binding>,
    row: Arc<[u8]>,
    scan: StreamingTableRowJob,
    columns: u32,
    expected_source_cells: u32,
    emitted: u32,
    complete: bool,
}

impl<Binding> TableBodyReplayJob<Binding> {
    pub const fn binding(&self) -> &Binding {
        self.binding
            .as_ref()
            .expect("active body replay retains its actor binding")
    }

    pub fn poll(
        &mut self,
        fuel: usize,
        cancellation: &CancellationToken,
    ) -> TableBodyReplayPoll<Binding> {
        assert!(fuel > 0);
        assert!(
            !self.complete,
            "body replay must not be polled after Complete"
        );
        if cancellation.is_cancelled() {
            return TableBodyReplayPoll::Cancelled { inspected: 0 };
        }
        match self.scan.poll(&self.row, fuel, cancellation) {
            TableRowStreamPoll::Pending { inspected } => TableBodyReplayPoll::Pending { inspected },
            TableRowStreamPoll::Cell { value, inspected } => {
                if self.emitted < self.columns {
                    let column = self.emitted;
                    self.emitted += 1;
                    TableBodyReplayPoll::Cell {
                        value: TableBodyReplayCell {
                            column,
                            cell: value.cell,
                        },
                        inspected,
                    }
                } else {
                    TableBodyReplayPoll::Pending { inspected }
                }
            }
            TableRowStreamPoll::Complete { value, inspected } => {
                let Some(summary) = value else {
                    return TableBodyReplayPoll::Failed {
                        error: TableReplayError::CertifiedRootsChanged,
                        inspected,
                    };
                };
                if summary.cells != self.expected_source_cells {
                    return TableBodyReplayPoll::Failed {
                        error: TableReplayError::CertifiedRootsChanged,
                        inspected,
                    };
                }
                self.complete = true;
                let emitted_cells = self.expected_source_cells.min(self.columns);
                TableBodyReplayPoll::Complete {
                    binding: self
                        .binding
                        .take()
                        .expect("body replay binding is returned exactly once"),
                    value: TableBodyReplaySummary {
                        source_cells: self.expected_source_cells,
                        emitted_cells,
                        padded_cells: self.columns.saturating_sub(emitted_cells),
                        ignored_cells: self.expected_source_cells.saturating_sub(self.columns),
                    },
                    inspected,
                }
            }
            TableRowStreamPoll::Cancelled { inspected } => {
                TableBodyReplayPoll::Cancelled { inspected }
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TableRowStreamPoll {
    Pending {
        inspected: usize,
    },
    Cell {
        value: TableRowStreamCell,
        inspected: usize,
    },
    Complete {
        value: Option<TableRowStreamSummary>,
        inspected: usize,
    },
    Cancelled {
        inspected: usize,
    },
}

impl TableRowStreamPoll {
    pub const fn inspected(&self) -> usize {
        match self {
            Self::Pending { inspected }
            | Self::Cell { inspected, .. }
            | Self::Complete { inspected, .. }
            | Self::Cancelled { inspected } => *inspected,
        }
    }
}

/// Physical-line table tokenization with constant retained state and at most
/// one output cell per poll. Paragraph-level table-header handoff can append
/// each cell directly into its candidate-owned persistent fragment, so neither
/// giant rows nor checkpoints retain a proportional vector.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamingTableRowJob {
    cursor: usize,
    end: usize,
    raw_start: usize,
    scan_start: usize,
    internal_offset: usize,
    backslash_run: usize,
    cell_had_escaped_pipe: bool,
    content_start: Option<usize>,
    content_end: usize,
    delimiter_valid: bool,
    delimiter_seen: bool,
    delimiter_has_hyphen: bool,
    delimiter_first: u8,
    delimiter_last: u8,
    delimiter_trailing_space: bool,
    leading_checked: bool,
    after_pipe_only_spaces: bool,
    rejected: bool,
    done: bool,
    cell_count: u32,
    all_cells_are_delimiters: bool,
    completion: Option<Option<TableRowStreamSummary>>,
    receipt: ScanReceipt,
}

impl StreamingTableRowJob {
    pub fn new(input: &[u8]) -> Self {
        Self {
            cursor: 0,
            end: physical_content_end(input),
            raw_start: 0,
            scan_start: 0,
            internal_offset: 0,
            backslash_run: 0,
            cell_had_escaped_pipe: false,
            content_start: None,
            content_end: 0,
            delimiter_valid: true,
            delimiter_seen: false,
            delimiter_has_hyphen: false,
            delimiter_first: 0,
            delimiter_last: 0,
            delimiter_trailing_space: false,
            leading_checked: false,
            after_pipe_only_spaces: false,
            rejected: false,
            done: false,
            cell_count: 0,
            all_cells_are_delimiters: true,
            completion: None,
            receipt: ScanReceipt::default(),
        }
    }

    pub const fn receipt(&self) -> ScanReceipt {
        self.receipt
    }

    /// No heap-backed collection is retained by the streaming scanner.
    pub const fn accounted_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
    }

    pub fn poll(
        &mut self,
        input: &[u8],
        fuel: usize,
        cancellation: &CancellationToken,
    ) -> TableRowStreamPoll {
        assert!(fuel > 0);
        assert_eq!(self.end, physical_content_end(input));
        assert!(
            !self.done || self.completion.is_some(),
            "StreamingTableRowJob must not be polled after Complete"
        );
        if cancellation.is_cancelled() {
            self.receipt.record_poll(0);
            return TableRowStreamPoll::Cancelled { inspected: 0 };
        }
        if self.done {
            self.receipt.record_poll(0);
            return TableRowStreamPoll::Complete {
                value: self
                    .completion
                    .take()
                    .expect("stream completion remains available once"),
                inspected: 0,
            };
        }
        let mut inspected = 0;
        while !self.done && inspected < fuel {
            if self.cursor == self.end {
                let cell = self.finish_at_end();
                self.receipt.record_poll(inspected);
                if let Some(value) = cell {
                    return TableRowStreamPoll::Cell { value, inspected };
                }
                return TableRowStreamPoll::Complete {
                    value: self
                        .completion
                        .take()
                        .expect("row end installs stream completion"),
                    inspected,
                };
            }
            let index = self.cursor;
            let byte = input[index];
            self.cursor += 1;
            inspected += 1;

            if !self.leading_checked {
                self.leading_checked = true;
                if byte == b'|' {
                    self.raw_start = 1;
                    self.scan_start = 1;
                    self.after_pipe_only_spaces = true;
                    continue;
                }
            }

            if self.after_pipe_only_spaces && is_table_space(byte) {
                self.scan_start = self.cursor;
                self.internal_offset += 1;
                continue;
            }
            self.after_pipe_only_spaces = false;

            if byte == b'\\' {
                self.observe_content(index, byte);
                self.backslash_run += 1;
                continue;
            }
            if byte == b'|' {
                // The generated repeated-token scanner retains a pipe as cell
                // content after any immediately preceding backslash run. The
                // donor materializer removes a slash only for an odd run.
                if self.backslash_run > 0 {
                    self.cell_had_escaped_pipe |= self.backslash_run % 2 == 1;
                    self.observe_content(index, byte);
                    self.backslash_run = 0;
                    continue;
                }
                let cell = self.finish_cell(self.raw_start..index);
                if self.rejected {
                    self.done = true;
                    self.complete_result();
                } else {
                    self.raw_start = index + 1;
                    self.scan_start = index + 1;
                    self.internal_offset = 0;
                    self.cell_had_escaped_pipe = false;
                    self.reset_cell_state(index + 1);
                    self.after_pipe_only_spaces = true;
                    self.backslash_run = 0;
                }
                self.receipt.record_poll(inspected);
                if let Some(value) = cell {
                    return TableRowStreamPoll::Cell { value, inspected };
                }
                return TableRowStreamPoll::Complete {
                    value: self
                        .completion
                        .take()
                        .expect("overflow installs stream completion"),
                    inspected,
                };
            }
            self.backslash_run = 0;
            self.observe_content(index, byte);
        }
        self.receipt.record_poll(inspected);
        if cancellation.is_cancelled() {
            TableRowStreamPoll::Cancelled { inspected }
        } else {
            TableRowStreamPoll::Pending { inspected }
        }
    }

    fn finish_at_end(&mut self) -> Option<TableRowStreamCell> {
        if !self.leading_checked && self.end == 0 {
            self.done = true;
            self.complete_result();
            return None;
        }
        let cell = if !self.after_pipe_only_spaces && self.raw_start < self.end {
            self.finish_cell(self.raw_start..self.end)
        } else if !self.after_pipe_only_spaces
            && self.raw_start == 0
            && self.end > 0
            && self.cell_count == 0
        {
            self.finish_cell(0..self.end)
        } else {
            None
        };
        self.done = true;
        self.complete_result();
        cell
    }

    fn finish_cell(&mut self, source: Range<usize>) -> Option<TableRowStreamCell> {
        let Some(next_count) = self.cell_count.checked_add(1) else {
            self.rejected = true;
            return None;
        };
        // `table_cell_end` owns horizontal table whitespace immediately
        // following a pipe. It remains inside the source span and contributes
        // to `internal_offset`, but is not part of materialized cell content.
        let content = self
            .content_start
            .map_or(self.scan_start..self.scan_start, |start| {
                start..self.content_end
            });
        let cell = CellSummary {
            source,
            content,
            internal_offset: self.internal_offset,
            had_escaped_pipe: self.cell_had_escaped_pipe,
        };

        let alignment = (self.delimiter_valid
            && self.delimiter_seen
            && self.delimiter_has_hyphen
            && !self.cell_had_escaped_pipe)
            .then_some(
                match (self.delimiter_first == b':', self.delimiter_last == b':') {
                    (false, false) => 0,
                    (true, false) => 1,
                    (true, true) => 2,
                    (false, true) => 3,
                },
            );
        if alignment.is_none() {
            self.all_cells_are_delimiters = false;
        }
        self.cell_count = next_count;
        Some(TableRowStreamCell {
            cell,
            delimiter_alignment: alignment,
        })
    }

    fn observe_content(&mut self, index: usize, byte: u8) {
        if is_cmark_space(byte) {
            self.delimiter_trailing_space |= self.content_start.is_some();
            return;
        }
        if self.content_start.is_none() {
            self.content_start = Some(index);
        }
        self.content_end = index + 1;
        if self.delimiter_trailing_space {
            self.delimiter_valid = false;
            self.delimiter_trailing_space = false;
        }
        if !self.delimiter_seen {
            self.delimiter_first = byte;
            self.delimiter_seen = true;
        }
        self.delimiter_last = byte;
        self.delimiter_has_hyphen |= byte == b'-';
        self.delimiter_valid &= matches!(byte, b'-' | b':');
    }

    fn reset_cell_state(&mut self, start: usize) {
        self.content_start = None;
        self.content_end = start;
        self.delimiter_valid = true;
        self.delimiter_seen = false;
        self.delimiter_has_hyphen = false;
        self.delimiter_first = 0;
        self.delimiter_last = 0;
        self.delimiter_trailing_space = false;
    }

    fn complete_result(&mut self) {
        if self.rejected || self.cell_count == 0 {
            self.completion = Some(None);
            return;
        }
        self.completion = Some(Some(TableRowStreamSummary {
            cells: self.cell_count,
            delimiter_row: self.all_cells_are_delimiters,
        }));
    }
}

/// Compatibility collector for differential comparison with the pinned
/// Comrak facade. Production must consume [`StreamingTableRowJob`] directly;
/// this wrapper deliberately preserves the donor facade's current `u16` cap.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableRowJob {
    stream: StreamingTableRowJob,
    cells: Vec<CellSummary>,
    delimiter_alignments: Vec<u8>,
    delimiter_candidates_valid: bool,
    rejected: bool,
    done: bool,
}

impl TableRowJob {
    pub fn new(input: &[u8]) -> Self {
        Self {
            stream: StreamingTableRowJob::new(input),
            cells: Vec::new(),
            delimiter_alignments: Vec::new(),
            delimiter_candidates_valid: true,
            rejected: false,
            done: false,
        }
    }

    pub const fn receipt(&self) -> ScanReceipt {
        self.stream.receipt()
    }

    pub fn poll(
        &mut self,
        input: &[u8],
        fuel: usize,
        cancellation: &CancellationToken,
    ) -> Poll<Option<TableRowSummary>> {
        assert!(!self.done, "TableRowJob must not be polled after Ready");
        match self.stream.poll(input, fuel, cancellation) {
            TableRowStreamPoll::Pending { inspected } => Poll::Pending { inspected },
            TableRowStreamPoll::Cancelled { inspected } => Poll::Cancelled { inspected },
            TableRowStreamPoll::Cell { value, inspected } => {
                if self.cells.len() == MAX_TABLE_CELLS {
                    self.rejected = true;
                } else if !self.rejected {
                    if self.delimiter_candidates_valid {
                        if let Some(alignment) = value.delimiter_alignment {
                            self.delimiter_alignments.push(alignment);
                        } else {
                            self.delimiter_candidates_valid = false;
                            self.delimiter_alignments.clear();
                        }
                    }
                    self.cells.push(value.cell);
                }
                Poll::Pending { inspected }
            }
            TableRowStreamPoll::Complete { value, inspected } => {
                self.done = true;
                let value = value.and_then(|summary| {
                    if self.rejected
                        || usize::try_from(summary.cells).ok() != Some(self.cells.len())
                        || summary.delimiter_row != self.delimiter_candidates_valid
                    {
                        return None;
                    }
                    Some(TableRowSummary {
                        cells: std::mem::take(&mut self.cells),
                        delimiter_alignments: summary
                            .delimiter_row
                            .then(|| std::mem::take(&mut self.delimiter_alignments)),
                    })
                });
                Poll::Ready { value, inspected }
            }
        }
    }
}

fn is_table_space(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | 0x0b | 0x0c)
}

fn is_cmark_space(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\r' | b'\n')
}
