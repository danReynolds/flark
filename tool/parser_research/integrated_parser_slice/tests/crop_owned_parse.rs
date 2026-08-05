#![cfg(feature = "crop-research")]

use std::sync::Arc;

use flark_integrated_parser_slice::block::{BlockJob, BlockStatus};
use flark_integrated_parser_slice::crop_source::{CropRangeDescriptor, CropSnapshotLease};
use flark_integrated_parser_slice::execution::{
    run_measured_activation, run_measured_slice, ActivationSliceReport, ExecutionSliceReport,
};
use flark_integrated_parser_slice::lifetime::PhysicalLifetime;
use flark_integrated_parser_slice::owned_parse::{
    decode_leaf_record, decode_manifest, OwnedParseJob, OwnedParseSummary, OWNED_PARSE_ANCHOR,
};
use flark_integrated_parser_slice::scheduler::{
    Admission, ArenaRootId, ParseSliceStatus, Scheduler, SliceLimits, SourceOperation,
    SourceRevision, SourceRootId,
};
use flark_integrated_parser_slice::source::PersistentSource;

fn limits() -> SliceLimits {
    SliceLimits {
        source_bytes: u64::MAX,
        transitions: u64::MAX,
        allocated_bytes: u64::MAX,
        copied_bytes: u64::MAX,
        hashed_bytes: u64::MAX,
        index_nodes: u64::MAX,
        reclaimed_nodes: 31,
    }
}

fn initialize() -> (PhysicalLifetime, Scheduler) {
    let mut lifetime = PhysicalLifetime::new();
    let initial = PersistentSource::from_text("");
    let (scheduler, _, _) = lifetime
        .initialize_scheduler(limits(), SourceRootId(initial.identity().0), 1, b"initial")
        .unwrap();
    (lifetime, scheduler)
}

struct Completed {
    summary: OwnedParseSummary,
    pages: Vec<Vec<u8>>,
}

fn run_job(
    target: SourceRootId,
    build: impl FnOnce(
        flark_integrated_parser_slice::scheduler::ParseToken,
        flark_integrated_parser_slice::scheduler::ArenaJobId,
    ) -> OwnedParseJob,
) -> Completed {
    let (mut lifetime, mut scheduler) = initialize();
    let submission = scheduler
        .submit_measured_source_operation(SourceOperation {
            base_revision: scheduler.source_revision(),
            target_revision: SourceRevision(scheduler.source_revision().0 + 1),
            base_root: scheduler.source_root(),
            result_root: target,
        })
        .unwrap();
    assert_eq!(submission.admission, Admission::Active);
    let ActivationSliceReport::Activated(activation) =
        run_measured_activation(&mut scheduler, &mut lifetime, OWNED_PARSE_ANCHOR).unwrap()
    else {
        panic!("active measured source must activate")
    };
    let mut job = build(submission.token, activation.job);
    for slices in 1..20_000_000 {
        let ExecutionSliceReport::Measured(report) =
            run_measured_slice(&mut scheduler, &mut lifetime, &mut job).unwrap()
        else {
            panic!("unexpected scheduler-only status")
        };
        if report.status == ParseSliceStatus::ReadyToSeal {
            let summary = job.summary().unwrap();
            let root = lifetime
                .seal_scheduler_job(
                    &mut scheduler,
                    submission.token,
                    job.visible_pages().unwrap(),
                )
                .unwrap();
            scheduler.commit_sealed(submission.token).unwrap();
            return Completed {
                summary,
                pages: root_chain(&lifetime, root),
            };
        }
        assert!(slices < 19_999_999, "owned parse did not converge");
    }
    unreachable!()
}

fn run_custom(text: &str) -> Completed {
    let source = Arc::new(PersistentSource::from_text(text));
    let identity = SourceRootId(source.identity().0);
    run_job(identity, move |token, arena| {
        OwnedParseJob::new(token, arena, source)
    })
}

fn run_crop(source: Arc<CropSnapshotLease>) -> Completed {
    let identity = SourceRootId(source.identity().0);
    run_job(identity, move |token, arena| {
        OwnedParseJob::new_crop(token, arena, source)
    })
}

fn root_chain(lifetime: &PhysicalLifetime, root: ArenaRootId) -> Vec<Vec<u8>> {
    let mut cursor = lifetime.root_chain_cursor(root).unwrap();
    let mut newest_first = Vec::new();
    while let Some(payload) = lifetime.root_chain_step(&mut cursor).unwrap() {
        newest_first.push(payload.to_vec());
    }
    newest_first.reverse();
    newest_first
}

fn assert_semantically_identical(left: &Completed, right: &Completed) {
    assert_eq!(left.summary.leaf_count, right.summary.leaf_count);
    assert_eq!(left.summary.span_count, right.summary.span_count);
    assert_eq!(
        left.summary.canonical_page_count,
        right.summary.canonical_page_count
    );
    assert_eq!(
        left.summary.canonical_payload_bytes,
        right.summary.canonical_payload_bytes
    );
    assert_eq!(
        left.summary.record_page_count,
        right.summary.record_page_count
    );
    assert_eq!(left.summary.semantic_digest, right.summary.semantic_digest);
    assert_eq!(left.pages.len(), right.pages.len());

    for (index, (left, right)) in left.pages.iter().zip(&right.pages).enumerate() {
        match (decode_leaf_record(left), decode_leaf_record(right)) {
            (Some(left), Some(right)) => {
                assert_eq!(left.ordinal, right.ordinal, "record page {index}");
                assert_eq!(
                    left.physical_start, right.physical_start,
                    "record page {index}"
                );
                assert_eq!(left.physical_end, right.physical_end, "record page {index}");
                assert_eq!(left.span_count, right.span_count, "record page {index}");
                assert_eq!(
                    left.canonical_page_count, right.canonical_page_count,
                    "record page {index}"
                );
                assert_eq!(
                    left.canonical_payload_bytes, right.canonical_payload_bytes,
                    "record page {index}"
                );
                assert_eq!(
                    left.inline_digest, right.inline_digest,
                    "record page {index}"
                );
                assert_eq!(
                    left.context_depth, right.context_depth,
                    "record page {index}"
                );
                assert_eq!(left.context, right.context, "record page {index}");
            }
            (None, None) => match (decode_manifest(left), decode_manifest(right)) {
                (Some(left), Some(right)) => {
                    assert_eq!(left.leaf_count, right.leaf_count);
                    assert_eq!(left.span_count, right.span_count);
                    assert_eq!(left.canonical_page_count, right.canonical_page_count);
                    assert_eq!(left.canonical_payload_bytes, right.canonical_payload_bytes);
                    assert_eq!(left.record_page_count, right.record_page_count);
                    assert_eq!(left.visible_pages, right.visible_pages);
                    assert_eq!(left.semantic_digest, right.semantic_digest);
                }
                (None, None) => assert_eq!(left, right, "canonical page {index}"),
                _ => panic!("page kind differs at {index}"),
            },
            _ => panic!("page kind differs at {index}"),
        }
    }
    assert_eq!(
        left.pages.first().map(Vec::as_slice),
        Some(OWNED_PARSE_ANCHOR)
    );
}

#[test]
fn crop_reuses_the_real_owned_pipeline_and_matches_every_canonical_byte() {
    let cases = [
        "plain `code` and *em*\r\n\r\n> quoted **strong**\r\n> continued `x`\r\n\r\n- λ🦀 _item_",
        "> - **alpha**\n>   beta and `gamma`\n\nlazy *tail*",
        "a \\*literal\\* and ***nested*** plus ``code``\n\nsecond _leaf_",
    ];
    for text in cases {
        let crop = CropSnapshotLease::from_text(text);
        let crop_result = run_crop(crop);
        let custom_result = run_custom(text);
        assert_semantically_identical(&crop_result, &custom_result);
    }
}

#[test]
fn adversarial_scalar_safe_edits_match_clean_custom_output() {
    let base = "first *one*\r\n\r\n> λ quoted `code`\r\n\r\n- last **two**";
    let edits = [
        (0..0, "intro _zero_\r\n\r\n"),
        (
            base.find("one").unwrap()..base.find("one").unwrap() + 3,
            "🦀",
        ),
        (
            base.find("> ").unwrap()..base.find("> ").unwrap() + 2,
            ">> ",
        ),
        (base.len() - 3..base.len(), "three `x`"),
    ];
    for (range, replacement) in edits {
        let base_source = CropSnapshotLease::from_text(base);
        let (edited, _) = base_source.edit(range, replacement).unwrap();
        let materialized = edited.materialize();
        assert_semantically_identical(&run_crop(edited), &run_custom(&materialized));
    }
}

#[test]
fn unchanged_suffix_convergence_uses_only_edit_provenance() {
    let text = "first *one*\r\n\r\nsecond λ `two`\r\n\r\nthird **three**";
    let old = CropSnapshotLease::from_text(text);
    let suffix_start = text.find("second").unwrap();
    let old_suffix = old.descriptor(suffix_start..text.len()).unwrap();
    let first = text.find("one").unwrap();
    let (new, provenance) = old.edit(first..first + 3, "🦀 crab").unwrap();
    let mapped = provenance
        .map_descriptor(old_suffix)
        .expect("operation proves suffix unchanged");
    assert_eq!(mapped.root, new.identity());
    assert_eq!(
        &old.materialize().as_bytes()[old_suffix.start..old_suffix.end],
        &new.materialize().as_bytes()[mapped.start..mapped.end]
    );
    assert!(provenance
        .map_unchanged(old.identity(), first.saturating_sub(1)..first + 4)
        .is_none());
    assert!(provenance
        .map_unchanged(new.identity(), suffix_start..text.len())
        .is_none());
    assert_semantically_identical(&run_crop(new.clone()), &run_custom(&new.materialize()));
}

#[test]
fn block_job_retains_no_per_leaf_crop_capture_and_preserves_crlf_utf8() {
    const fn require_copy<T: Copy>() {}
    require_copy::<CropRangeDescriptor>();
    assert!(std::mem::size_of::<CropRangeDescriptor>() <= 3 * std::mem::size_of::<usize>());

    let text = (0..10_000)
        .map(|index| format!("leaf {index} λ🦀 *x*"))
        .collect::<Vec<_>>()
        .join("\r\n\r\n");
    let source = CropSnapshotLease::from_text(&text);
    let mut job = BlockJob::new_crop(source);
    loop {
        let poll = job.poll(4_096);
        match poll.status {
            BlockStatus::Pending => {}
            BlockStatus::Ready => break,
            BlockStatus::Failed => panic!("Crop block job failed: {:?}", job.error()),
        }
    }
    let output = job.result().unwrap();
    let receipt = output.receipt();
    assert_eq!(output.len(), 10_000);
    assert_eq!(receipt.source_bytes_inspected, text.len());
    assert_eq!(receipt.source_fragment_nodes_allocated, 0);
    assert_eq!(receipt.source_fragment_handles_retained, 0);
    assert_eq!(receipt.source_capture_piece_runs, 0);
    assert_eq!(receipt.source_capture_buffer_handle_clones, 0);
    assert_eq!(receipt.source_fragment_payload_bytes_copied, 0);
    assert!(receipt.source_chunk_loads > 0);
    assert!(receipt.source_chunk_bytes_copied >= text.len());
    assert!(receipt.source_chunk_bytes_copied < text.len() + 4_096);
    assert!(receipt.max_atomic_source_chunk_bytes_copied <= 4_096);
}
