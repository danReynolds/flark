use std::hint::black_box;
use std::time::{Duration, Instant};

use comrak::block_spine_facade as facade;
use flark_oversized_block_line_gate::{
    AtxTailJob, CancellationToken, DEFAULT_POLL_BYTES, FenceJob, FenceMode, HtmlEndJob,
    HtmlType7Job, MAX_TABLE_CELLS, MarkerLineJob, Poll, ReferencePrefixJob, ScanReceipt,
    TableRowJob, TableRowSummary, run_to_ready,
};
use serde_json::{Value, json};

fn main() {
    let mut large = Vec::new();
    for mib in [1, 10] {
        let bytes = mib * 1024 * 1024;
        large.extend(large_receipts(bytes));
    }

    let output = json!({
        "schema": 1,
        "mode": "all-sizes-resumable",
        "poll_bytes": DEFAULT_POLL_BYTES,
        "large_lines": large,
        "ordinary_line_overhead": ordinary_overhead(),
        "cancellation": cancellation_receipt(10 * 1024 * 1024),
    });
    println!("{}", serde_json::to_string_pretty(&output).unwrap());
}

fn large_receipts(bytes: usize) -> Vec<Value> {
    let mut rows = Vec::new();

    let input = padded(b"```", b'a', b"\n", bytes);
    let token = CancellationToken::default();
    let mut job = FenceJob::new(&input, FenceMode::Open);
    rows.push(measure("fence_open_backtick", bytes, || {
        let (value, receipt) =
            run_to_ready(|| job.poll(&input, DEFAULT_POLL_BYTES, &token)).unwrap();
        assert_eq!(value, Some(3));
        receipt
    }));

    let input = padded(b"```", b' ', b"\n", bytes);
    let token = CancellationToken::default();
    let mut job = FenceJob::new(&input, FenceMode::Close);
    rows.push(measure("fence_close_spaces", bytes, || {
        let (value, receipt) =
            run_to_ready(|| job.poll(&input, DEFAULT_POLL_BYTES, &token)).unwrap();
        assert_eq!(value, Some(3));
        receipt
    }));

    let input = padded(b"", b'-', b"\n", bytes);
    let token = CancellationToken::default();
    let mut job = MarkerLineJob::new(&input);
    rows.push(measure("setext_thematic", bytes, || {
        let (value, receipt) =
            run_to_ready(|| job.poll(&input, DEFAULT_POLL_BYTES, &token)).unwrap();
        assert_eq!(value.setext, Some(b'-'));
        assert!(value.thematic_break);
        receipt
    }));

    let input = padded(b"body ", b'x', b" ###   \n", bytes);
    let token = CancellationToken::default();
    let mut job = AtxTailJob::new(&input);
    rows.push(measure("atx_tail", bytes, || {
        let (value, receipt) =
            run_to_ready(|| job.poll(&input, DEFAULT_POLL_BYTES, &token)).unwrap();
        assert!(value.closed);
        receipt
    }));

    let input = padded(b"", b'x', b"-->\n", bytes);
    let token = CancellationToken::default();
    let mut job = HtmlEndJob::new(&input, 2);
    rows.push(measure("html_comment_end", bytes, || {
        let (value, receipt) =
            run_to_ready(|| job.poll(&input, DEFAULT_POLL_BYTES, &token)).unwrap();
        assert!(value);
        receipt
    }));

    let input = padded(b"<x a=\"", b'v', b"\">\n", bytes);
    let token = CancellationToken::default();
    let mut job = HtmlType7Job::new(&input);
    rows.push(measure("html_type7", bytes, || {
        let (value, receipt) =
            run_to_ready(|| job.poll(&input, DEFAULT_POLL_BYTES, &token)).unwrap();
        assert!(value);
        receipt
    }));

    let input = padded(b"| ", b'x', b" | tail |\n", bytes);
    let token = CancellationToken::default();
    let mut job = TableRowJob::new(&input);
    rows.push(measure("table_row", bytes, || {
        let (value, receipt) =
            run_to_ready(|| job.poll(&input, DEFAULT_POLL_BYTES, &token)).unwrap();
        assert_eq!(value.unwrap().cells.len(), 2);
        receipt
    }));

    rows.push(measure_dense_table(bytes, false));
    rows.push(measure_dense_table(bytes, true));

    let input = padded(b"[x]: /", b'u', b"\n", bytes);
    let token = CancellationToken::default();
    let mut job = ReferencePrefixJob::new();
    rows.push(measure("reference_definition", bytes, || {
        let (value, receipt) =
            run_to_ready(|| job.poll(&input, DEFAULT_POLL_BYTES, &token)).unwrap();
        assert_eq!(value.unwrap().source.end, input.len());
        receipt
    }));

    rows
}

fn measure_dense_table(bytes: usize, over_cell_cap: bool) -> Value {
    let input = dense_table_input(bytes, over_cell_cap);
    let token = CancellationToken::default();
    let mut job = TableRowJob::new(&input);
    let started = Instant::now();
    let mut maximum_poll_ns = 0_u128;
    let value = loop {
        let poll_started = Instant::now();
        let poll = job.poll(&input, DEFAULT_POLL_BYTES, &token);
        maximum_poll_ns = maximum_poll_ns.max(poll_started.elapsed().as_nanos());
        match poll {
            Poll::Pending { .. } => {}
            Poll::Ready { value, .. } => break value,
            Poll::Cancelled { .. } => panic!("unexpected cancellation"),
        }
    };
    let elapsed = started.elapsed();
    let receipt = job.receipt();
    assert!(receipt.maximum_bytes_per_poll <= DEFAULT_POLL_BYTES);
    if over_cell_cap {
        assert!(value.is_none());
    } else {
        assert_eq!(value.as_ref().unwrap().cells.len(), MAX_TABLE_CELLS);
    }
    json!({
        "classifier": if over_cell_cap { "table_row_over_cell_cap" } else { "table_row_max_cells" },
        "input_bytes": bytes,
        "elapsed_us": elapsed.as_micros(),
        "maximum_poll_us": maximum_poll_ns.div_ceil(1000),
        "polls": receipt.polls,
        "bytes_inspected": receipt.bytes_inspected,
        "maximum_bytes_per_poll": receipt.maximum_bytes_per_poll,
        "cancellation_checks": receipt.cancellation_checks,
        "output_accounted_bytes": value.as_ref().map_or(0, TableRowSummary::accounted_bytes),
    })
}

fn measure(name: &str, input_bytes: usize, run: impl FnOnce() -> ScanReceipt) -> Value {
    let started = Instant::now();
    let receipt = run();
    let elapsed = started.elapsed();
    assert!(receipt.maximum_bytes_per_poll <= DEFAULT_POLL_BYTES);
    json!({
        "classifier": name,
        "input_bytes": input_bytes,
        "elapsed_us": elapsed.as_micros(),
        "polls": receipt.polls,
        "bytes_inspected": receipt.bytes_inspected,
        "maximum_bytes_per_poll": receipt.maximum_bytes_per_poll,
        "cancellation_checks": receipt.cancellation_checks,
    })
}

fn cancellation_receipt(bytes: usize) -> Value {
    let input = padded(b"<x a=\"", b'v', b"\">\n", bytes);
    let token = CancellationToken::default();
    let mut job = HtmlType7Job::new(&input);
    let first = job.poll(&input, DEFAULT_POLL_BYTES, &token);
    assert!(matches!(
        first,
        Poll::Pending {
            inspected: DEFAULT_POLL_BYTES
        }
    ));
    token.cancel();
    let started = Instant::now();
    let second = job.poll(&input, DEFAULT_POLL_BYTES, &token);
    let elapsed = started.elapsed();
    assert!(matches!(second, Poll::Cancelled { inspected: 0 }));
    json!({
        "input_bytes": bytes,
        "bytes_before_cancel": first.inspected(),
        "bytes_after_cancel": second.inspected(),
        "cancel_observation_us": elapsed.as_micros(),
        "maximum_bytes_per_poll": job.receipt().maximum_bytes_per_poll,
    })
}

fn ordinary_overhead() -> Vec<Value> {
    const ITERATIONS: usize = 50_000;
    let token = CancellationToken::default();
    let mut rows = Vec::new();

    let input = format!("``` {}\n", "language".repeat(8));
    rows.push(compare_small(
        "fence_open_backtick",
        ITERATIONS,
        || {
            black_box(facade::open_code_fence(black_box(&input)).unwrap());
        },
        || {
            let mut job = FenceJob::new(black_box(input.as_bytes()), FenceMode::Open);
            black_box(
                run_to_ready(|| job.poll(input.as_bytes(), DEFAULT_POLL_BYTES, &token))
                    .unwrap()
                    .0,
            );
        },
    ));

    let input = "<widget class=\"primary\" data-id=42>\n";
    rows.push(compare_small(
        "html_type7",
        ITERATIONS,
        || {
            black_box(facade::html_block_start(black_box(input), true).unwrap());
        },
        || {
            let mut job = HtmlType7Job::new(black_box(input.as_bytes()));
            black_box(
                run_to_ready(|| job.poll(input.as_bytes(), DEFAULT_POLL_BYTES, &token))
                    .unwrap()
                    .0,
            );
        },
    ));

    let input = "| alpha | beta | gamma |\n";
    rows.push(compare_small(
        "table_row",
        ITERATIONS,
        || {
            black_box(facade::table_row(black_box(input), false).unwrap());
        },
        || {
            let mut job = TableRowJob::new(black_box(input.as_bytes()));
            black_box(
                run_to_ready(|| job.poll(input.as_bytes(), DEFAULT_POLL_BYTES, &token))
                    .unwrap()
                    .0,
            );
        },
    ));

    let input = "[label]: /destination \"title\"\n";
    rows.push(compare_small(
        "reference_definition_shape",
        ITERATIONS,
        || {
            black_box(facade::reference_definitions(black_box(input)).unwrap());
        },
        || {
            let mut job = ReferencePrefixJob::new();
            black_box(
                run_to_ready(|| job.poll(input.as_bytes(), DEFAULT_POLL_BYTES, &token))
                    .unwrap()
                    .0,
            );
        },
    ));

    rows
}

fn compare_small(
    classifier: &str,
    iterations: usize,
    mut donor: impl FnMut(),
    mut resumable: impl FnMut(),
) -> Value {
    donor();
    resumable();
    let donor_ns = median_ns(iterations, &mut donor);
    let resumable_ns = median_ns(iterations, &mut resumable);
    json!({
        "classifier": classifier,
        "iterations_per_trial": iterations,
        "donor_ns_per_op": donor_ns,
        "resumable_ns_per_op": resumable_ns,
        "resumable_over_donor": resumable_ns / donor_ns.max(0.001),
    })
}

fn median_ns(iterations: usize, run: &mut impl FnMut()) -> f64 {
    let mut trials = Vec::with_capacity(5);
    for _ in 0..5 {
        let started = Instant::now();
        for _ in 0..iterations {
            run();
        }
        trials.push(started.elapsed());
    }
    trials.sort_unstable();
    duration_ns(trials[trials.len() / 2])
        / f64::from(u32::try_from(iterations).expect("benchmark iterations fit u32"))
}

fn duration_ns(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1_000_000_000.0
}

fn padded(prefix: &[u8], fill: u8, suffix: &[u8], bytes: usize) -> Vec<u8> {
    assert!(prefix.len() + suffix.len() <= bytes);
    let mut output = Vec::with_capacity(bytes);
    output.extend_from_slice(prefix);
    output.resize(bytes - suffix.len(), fill);
    output.extend_from_slice(suffix);
    output
}

fn dense_table_input(bytes: usize, over_cell_cap: bool) -> Vec<u8> {
    let closed_cells = if over_cell_cap {
        MAX_TABLE_CELLS + 1
    } else {
        MAX_TABLE_CELLS - 1
    };
    assert!(closed_cells * 2 + 2 <= bytes);
    let mut output = Vec::with_capacity(bytes);
    for _ in 0..closed_cells {
        output.extend_from_slice(b"x|");
    }
    output.resize(bytes - 1, b'x');
    output.push(b'\n');
    output
}
