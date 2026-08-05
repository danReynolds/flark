//! Regression coverage for physical lines that exceed the segmented
//! controller window (`DIRECT_SEGMENTED_LINE_WINDOW_BYTES`, 4 KiB).
//!
//! A truncated controller window used to be admitted only at the quiescent
//! document root, so any over-window physical line that followed a non-blank
//! line (paragraph continuation, the line after a reference definition, the
//! line after an ATX heading) failed closed with
//! `DirectUnsupported::SegmentedLine`. The candidate endpoint has no fallback
//! for that error, so it surfaced to callers as `Status::InternalFault`.
//!
//! Two over-window shapes are still deliberately unsupported and are therefore
//! not covered here; both predate this regression and are tracked separately:
//!   * a first nonspace byte that can open a block whose decision needs the
//!     omitted suffix (`> # ` ~ < - _ * + 0-9`, plus `= | :` under an open
//!     paragraph) — e.g. a bullet item longer than the window; and
//!   * a lazy continuation into an open list item.

use flark_engine::parser_internal::{M11RecursiveGreenPoint, M11RecursiveGreenRowQueryLimits};
use flark_engine::{DocumentRuntime, DocumentRuntimeConfig, SourceBoundaryAffinity};
use flark_parser::{M11PersistentRecursiveGreenBuildStatus, M11PersistentRecursiveGreenCleanPlan};

const WINDOW: usize = 4 * 1024;

#[derive(Debug, Eq, PartialEq)]
struct Observed {
    checkpoints: usize,
    references: u64,
    rows: Vec<(u16, Option<std::ops::Range<u64>>)>,
}

/// Cold-open clean build, then one retained renderable-row query over the
/// whole document.
fn observe(source: &str) -> Observed {
    let mut runtime =
        DocumentRuntime::new(source, DocumentRuntimeConfig::default()).expect("runtime");
    let plan = M11PersistentRecursiveGreenCleanPlan::new(
        runtime.snapshot_current_source().expect("scanner lease"),
        runtime.snapshot_current_source().expect("writer lease"),
        1,
    )
    .expect("clean plan");
    let mut build = plan.begin(&mut runtime).expect("clean build");
    loop {
        let poll = build
            .poll(&mut runtime, 64)
            .unwrap_or_else(|error| panic!("clean build of {} bytes: {error:?}", source.len()));
        if poll.status() == M11PersistentRecursiveGreenBuildStatus::Complete {
            break;
        }
    }
    let mut session = build.take_session().expect("persistent session");
    let window = session
        .query_renderable_rows(
            &runtime,
            M11RecursiveGreenPoint::new(0, 0, SourceBoundaryAffinity::After),
            source.len() as u64,
            M11RecursiveGreenRowQueryLimits::new(64, 64, 4096, 64, 4096).expect("row limits"),
        )
        .expect("renderable row query");
    let observed = Observed {
        checkpoints: session.checkpoint_count(),
        references: session.reference_occurrence_count(),
        rows: window
            .rows()
            .iter()
            .map(|row| (row.kind().get(), row.editable_range()))
            .collect(),
    };
    session.begin_release(&mut runtime).expect("begin release");
    while !session.poll_release(&mut runtime, 64).expect("poll release") {}
    runtime.begin_close().expect("begin close");
    while !runtime.poll_close(64).expect("close poll").complete {}
    observed
}

/// An over-window document must agree with the identical document whose final
/// physical line is one byte under the window, modulo the byte delta on the
/// last row.
fn assert_over_window_matches_under_window(prefix: &str, tail: usize) {
    let under_tail = WINDOW - 1 - "\n".len();
    let under = observe(&format!("{prefix}{}\n", "x".repeat(under_tail)));
    let over = observe(&format!("{prefix}{}\n", "x".repeat(tail)));
    let delta = (tail - under_tail) as u64;

    assert_eq!(
        over.checkpoints, under.checkpoints,
        "{prefix:?}+{tail}: checkpoint count",
    );
    assert_eq!(
        over.references, under.references,
        "{prefix:?}+{tail}: reference occurrences",
    );
    assert_eq!(
        over.rows.len(),
        under.rows.len(),
        "{prefix:?}+{tail}: row count ({:?} vs {:?})",
        over.rows,
        under.rows,
    );
    for (index, (over_row, under_row)) in over.rows.iter().zip(under.rows.iter()).enumerate() {
        assert_eq!(over_row.0, under_row.0, "{prefix:?}+{tail}: row {index} kind");
        let last = index + 1 == over.rows.len();
        match (&over_row.1, &under_row.1) {
            (Some(over_range), Some(under_range)) => {
                assert_eq!(
                    over_range.start, under_range.start,
                    "{prefix:?}+{tail}: row {index} start",
                );
                let expected_end = under_range.end + if last { delta } else { 0 };
                assert_eq!(
                    over_range.end, expected_end,
                    "{prefix:?}+{tail}: row {index} end",
                );
            }
            (None, None) => {}
            _ => panic!("{prefix:?}+{tail}: row {index} edit capability diverged"),
        }
    }
}

#[test]
fn over_window_physical_line_after_a_non_blank_line_parses_exactly() {
    for prefix in [
        "",
        "hello\n",
        "hello\n\n",
        "[a]: /u\n",
        "[a]: /u\n\n",
        "[a]: /u\n[b]: /x\n",
        "# heading\n",
        "> quoted\n",
        "```\nfenced\n```\n",
    ] {
        for tail in [WINDOW - 1, WINDOW, WINDOW + 1, 8 * WINDOW] {
            assert_over_window_matches_under_window(prefix, tail);
        }
    }
}

#[test]
fn reported_repro_shapes_complete_and_retain_their_definitions() {
    for source in [
        format!("[a]: /u\n{}", "x".repeat(4096)),
        format!("[a]: /u\n{}", "x".repeat(65536)),
        format!("[a]: /u\n[b]: /x\n{}", "x".repeat(65536)),
    ] {
        let observed = observe(&source);
        assert!(
            observed.references >= 1,
            "{}: definitions survive the long line",
            source.len(),
        );
        let last = observed.rows.last().expect("terminal row");
        assert_eq!(
            last.1.as_ref().map(|range| range.end),
            Some(source.len() as u64),
            "{}: terminal row covers the long line",
            source.len(),
        );
    }
}
