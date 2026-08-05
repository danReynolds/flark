use std::sync::Arc;

use flark_oversized_block_line_gate::{
    CancellationToken, MAX_TABLE_CELLS, TableBodyDisposition, TableBodyPassOneJob,
    TableBodyPassOnePoll, TableBodyRejectReason, TableBodyReplayJob, TableBodyReplayPoll,
    TableHeaderDisposition, TableHeaderPassOneJob, TableHeaderPassOnePoll, TableHeaderRejectReason,
    TableHeaderReplayJob, TableHeaderReplayPoll, ValidatedTableBodyRow, ValidatedTableHeader,
};

#[derive(Debug, PartialEq, Eq)]
struct Binding(u64);

fn root(bytes: impl AsRef<[u8]>) -> Arc<[u8]> {
    Arc::from(bytes.as_ref())
}

fn validate_header(
    binding: Binding,
    header: Arc<[u8]>,
    delimiter: Arc<[u8]>,
    fuel: usize,
) -> TableHeaderDisposition<Binding> {
    let cancellation = CancellationToken::default();
    let mut job = TableHeaderPassOneJob::new(binding, header, delimiter);
    loop {
        match job.poll(fuel, &cancellation) {
            TableHeaderPassOnePoll::Pending { inspected } => assert!(inspected <= fuel),
            TableHeaderPassOnePoll::Complete { value, inspected } => {
                assert!(inspected <= fuel);
                return value;
            }
            TableHeaderPassOnePoll::Cancelled { .. } => panic!("uncancelled validation cancelled"),
        }
    }
}

fn ready_header(
    binding: Binding,
    header: impl AsRef<[u8]>,
    delimiter: impl AsRef<[u8]>,
) -> ValidatedTableHeader<Binding> {
    match validate_header(binding, root(header), root(delimiter), 1) {
        TableHeaderDisposition::Ready(ready) => ready,
        outcome => panic!("expected validated Table header, got {outcome:?}"),
    }
}

fn validate_body(
    binding: Binding,
    row: impl AsRef<[u8]>,
    columns: u32,
) -> TableBodyDisposition<Binding> {
    let cancellation = CancellationToken::default();
    let mut job = TableBodyPassOneJob::new(binding, root(row), columns);
    loop {
        match job.poll(1, &cancellation) {
            TableBodyPassOnePoll::Pending { inspected } => assert!(inspected <= 1),
            TableBodyPassOnePoll::Complete { value, inspected } => {
                assert!(inspected <= 1);
                return value;
            }
            TableBodyPassOnePoll::Cancelled { .. } => panic!("uncancelled validation cancelled"),
        }
    }
}

fn ready_body(
    binding: Binding,
    row: impl AsRef<[u8]>,
    columns: u32,
) -> ValidatedTableBodyRow<Binding> {
    match validate_body(binding, row, columns) {
        TableBodyDisposition::Ready(ready) => ready,
        outcome => panic!("expected validated Table body, got {outcome:?}"),
    }
}

fn repeated_row(cell: u8, cells: usize) -> Arc<[u8]> {
    let mut bytes = Vec::with_capacity(cells.saturating_mul(2));
    for index in 0..cells {
        if index != 0 {
            bytes.push(b'|');
        }
        bytes.push(cell);
    }
    bytes.push(b'\n');
    Arc::from(bytes)
}

#[test]
fn pass_one_preserves_retry_vs_reject_and_mints_ready_only_after_a_count_match() {
    let ready = validate_header(Binding(1), root(b"a | b\n"), root(b"- | :-:\n"), 1);
    let TableHeaderDisposition::Ready(ready) = ready else {
        panic!("matching header was not ready")
    };
    assert_eq!(ready.binding(), &Binding(1));
    assert_eq!(ready.columns(), 2);

    assert!(matches!(
        validate_header(Binding(2), root(b"a | b\n"), root(b"ordinary text\n"), 1),
        TableHeaderDisposition::NotCandidate {
            binding: Binding(2)
        }
    ));
    assert!(matches!(
        validate_header(Binding(3), root(b"a | b\n"), root(b"- | - | -\n"), 1),
        TableHeaderDisposition::Rejected {
            binding: Binding(3),
            reason: TableHeaderRejectReason::ColumnCountMismatch,
        }
    ));
    assert!(matches!(
        validate_header(Binding(4), root(b""), root(b"-\n"), 1),
        TableHeaderDisposition::Rejected {
            binding: Binding(4),
            reason: TableHeaderRejectReason::HeaderNotRow,
        }
    ));
}

#[test]
fn first_pass_accepts_65535_columns_and_rejects_65536_before_replay() {
    let maximum = repeated_row(b'a', MAX_TABLE_CELLS);
    let maximum_delimiter = repeated_row(b'-', MAX_TABLE_CELLS);
    let ready = validate_header(Binding(5), maximum, maximum_delimiter, 4096);
    assert!(matches!(
        ready,
        TableHeaderDisposition::Ready(ref ready)
            if usize::try_from(ready.columns()) == Ok(MAX_TABLE_CELLS)
    ));

    let overflow_delimiter = repeated_row(b'-', MAX_TABLE_CELLS + 1);
    assert!(matches!(
        validate_header(Binding(6), root(b"unused\n"), overflow_delimiter, 4096),
        TableHeaderDisposition::Rejected {
            binding: Binding(6),
            reason: TableHeaderRejectReason::TooManyColumns,
        }
    ));
}

fn replay_header(
    mut replay: TableHeaderReplayJob<Binding>,
) -> (Binding, Vec<(u32, u8, std::ops::Range<usize>)>) {
    let cancellation = CancellationToken::default();
    let mut cells = Vec::new();
    loop {
        match replay.poll(1, &cancellation) {
            TableHeaderReplayPoll::Pending { inspected } => assert!(inspected <= 1),
            TableHeaderReplayPoll::Cell { value, inspected } => {
                assert!(inspected <= 1);
                cells.push((
                    value.column(),
                    value.alignment(),
                    value.header().content.clone(),
                ));
            }
            TableHeaderReplayPoll::Complete { binding, inspected } => {
                assert!(inspected <= 1);
                return (binding, cells);
            }
            TableHeaderReplayPoll::Cancelled { .. } => panic!("uncancelled replay cancelled"),
            TableHeaderReplayPoll::Failed { error, .. } => {
                panic!("validated replay failed: {error:?}")
            }
        }
    }
}

#[test]
fn pass_two_replays_fresh_paired_cells_under_fuel_one_and_cancels_without_another_read() {
    let ready = ready_header(Binding(7), b" left | right \n", b":- | -:\n");
    let mut replay = ready.into_replay();
    let cancellation = CancellationToken::default();
    assert!(matches!(
        replay.poll(1, &cancellation),
        TableHeaderReplayPoll::Pending { inspected: 1 }
    ));
    cancellation.cancel();
    assert!(matches!(
        replay.poll(1, &cancellation),
        TableHeaderReplayPoll::Cancelled { inspected: 0 }
    ));

    let ready = ready_header(Binding(8), b" left | right \n", b":- | -:\n");
    let (binding, cells) = replay_header(ready.into_replay());
    assert_eq!(binding, Binding(8));
    assert_eq!(cells.len(), 2);
    assert_eq!(cells[0].0, 0);
    assert_eq!(cells[0].1, 1);
    assert_eq!(cells[0].2, 1..5);
    assert_eq!(cells[1].0, 1);
    assert_eq!(cells[1].1, 3);
    assert_eq!(cells[1].2, 8..13);
}

fn replay_body(mut replay: TableBodyReplayJob<Binding>) -> (Binding, Vec<u32>, u32, u32) {
    let cancellation = CancellationToken::default();
    let mut columns = Vec::new();
    loop {
        match replay.poll(1, &cancellation) {
            TableBodyReplayPoll::Pending { inspected } => assert!(inspected <= 1),
            TableBodyReplayPoll::Cell { value, inspected } => {
                assert!(inspected <= 1);
                columns.push(value.column());
            }
            TableBodyReplayPoll::Complete {
                binding,
                value,
                inspected,
            } => {
                assert!(inspected <= 1);
                return (binding, columns, value.padded_cells, value.ignored_cells);
            }
            TableBodyReplayPoll::Cancelled { .. } => panic!("uncancelled body replay cancelled"),
            TableBodyReplayPoll::Failed { error, .. } => {
                panic!("validated body replay failed: {error:?}")
            }
        }
    }
}

#[test]
fn body_rows_validate_before_replay_then_bound_output_to_the_table_width() {
    let wide = ready_body(Binding(9), b"one | two | ignored\n", 2);
    assert_eq!(wide.source_cells(), 3);
    let (binding, columns, padded, ignored) = replay_body(wide.into_replay());
    assert_eq!(binding, Binding(9));
    assert_eq!(columns, [0, 1]);
    assert_eq!(padded, 0);
    assert_eq!(ignored, 1);

    let short = ready_body(Binding(10), b"one\n", 3);
    let (_, columns, padded, ignored) = replay_body(short.into_replay());
    assert_eq!(columns, [0]);
    assert_eq!(padded, 2);
    assert_eq!(ignored, 0);

    assert!(matches!(
        validate_body(Binding(11), b"", 2),
        TableBodyDisposition::NotRow {
            binding: Binding(11)
        }
    ));

    assert!(matches!(
        validate_body(Binding(12), repeated_row(b'x', MAX_TABLE_CELLS + 1), 2),
        TableBodyDisposition::Rejected {
            binding: Binding(12),
            reason: TableBodyRejectReason::TooManyCells,
        }
    ));
}
