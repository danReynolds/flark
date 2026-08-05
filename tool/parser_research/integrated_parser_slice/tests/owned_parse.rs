use std::sync::Arc;
use std::time::{Duration, Instant};

use flark_integrated_parser_slice::block::{BlockJob, BlockStatus};
use flark_integrated_parser_slice::execution::{
    run_measured_activation, run_measured_slice, ActivationSliceReport, ExecutionSliceReport,
};
use flark_integrated_parser_slice::frontier::{LexerStatus, SharedLexer};
use flark_integrated_parser_slice::inline_machine::{
    InlineMachine, InlineOutputPageDrainStep, InlineStatus, InlineWork,
};
use flark_integrated_parser_slice::lifetime::{LifetimeError, PhysicalLifetime};
use flark_integrated_parser_slice::owned_parse::{
    decode_leaf_record, decode_manifest, OwnedParseJob, OwnedParsePhase, OwnedParseSummary,
    OwnedParseTelemetry, OWNED_PARSE_ANCHOR,
};
use flark_integrated_parser_slice::scheduler::{
    Admission, ArenaJobId, ArenaRootId, ParseSliceStatus, ParseToken, Scheduler, SliceLimits,
    SourceOperation, SourceRevision, SourceRootId,
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

fn next_operation(scheduler: &Scheduler, source: &PersistentSource) -> SourceOperation {
    SourceOperation {
        base_revision: scheduler.source_revision(),
        target_revision: SourceRevision(scheduler.source_revision().0 + 1),
        base_root: scheduler.source_root(),
        result_root: SourceRootId(source.identity().0),
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
    lifetime: PhysicalLifetime,
    root: ArenaRootId,
    summary: OwnedParseSummary,
    telemetry: OwnedParseTelemetry,
    slices: usize,
}

fn run_owned(source: Arc<PersistentSource>) -> Completed {
    let (mut lifetime, mut scheduler) = initialize();
    let submission = scheduler
        .submit_measured_source_operation(next_operation(&scheduler, &source))
        .unwrap();
    assert_eq!(submission.admission, Admission::Active);
    let ActivationSliceReport::Activated(activation) =
        run_measured_activation(&mut scheduler, &mut lifetime, OWNED_PARSE_ANCHOR).unwrap()
    else {
        panic!("active measured source must activate")
    };
    let mut job = OwnedParseJob::new(submission.token, activation.job, source);
    let mut slices = 0;
    loop {
        slices += 1;
        assert!(slices < 20_000_000, "owned parse failed to converge");
        match run_measured_slice(&mut scheduler, &mut lifetime, &mut job).unwrap() {
            ExecutionSliceReport::Measured(report) => {
                assert!(report.audit.fits(scheduler.slice_limits()));
                if report.status == ParseSliceStatus::ReadyToSeal {
                    break;
                }
            }
            ExecutionSliceReport::Status(report) => {
                panic!("unexpected scheduler-only status while parsing: {report:?}")
            }
        }
    }
    let summary = job.summary().expect("ready job exposes summary");
    let visible_pages = job.visible_pages().expect("visible page count fits u32");
    assert_eq!(u64::from(visible_pages), summary.visible_pages());
    assert_eq!(
        lifetime.active_job_pages(),
        usize::try_from(summary.visible_pages()).unwrap() + 1,
        "physical chain adds exactly one activation anchor"
    );
    let root = lifetime
        .seal_scheduler_job(&mut scheduler, submission.token, visible_pages)
        .unwrap();
    scheduler.commit_sealed(submission.token).unwrap();
    let telemetry = job.telemetry();
    Completed {
        lifetime,
        root,
        summary,
        telemetry,
        slices,
    }
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

#[derive(Debug)]
struct DirectLeaf {
    physical_start: u64,
    physical_end: u64,
    context: Vec<[u8; 3]>,
    span_count: u64,
    payload_bytes: u64,
    digest: u64,
    pages: Vec<Vec<u8>>,
}

fn direct_leaves(source: Arc<PersistentSource>) -> Vec<DirectLeaf> {
    let mut block = BlockJob::new(source);
    loop {
        match block.poll(4096).status {
            BlockStatus::Pending => {}
            BlockStatus::Ready => break,
            BlockStatus::Failed => panic!("direct block parse failed: {:?}", block.error()),
        }
    }
    block
        .result()
        .unwrap()
        .leaves()
        .map(|leaf| {
            let mut lexer = SharedLexer::new(&leaf.input);
            while lexer.poll(4096).status != LexerStatus::Ready {}
            let mut inline = InlineMachine::new(lexer.consumers().unwrap().inline);
            loop {
                let poll = inline.poll(InlineWork::uniform(4096));
                if poll.status == InlineStatus::Ready {
                    break;
                }
                assert_ne!(poll.delta.transitions, 0);
            }
            let mut drain = inline.take_output().unwrap().into_page_drain();
            let span_count = u64::try_from(drain.span_count()).unwrap();
            let payload_bytes = u64::try_from(drain.payload_bytes()).unwrap();
            let digest = drain.digest();
            let mut pages = Vec::new();
            while let InlineOutputPageDrainStep::Page(page) = drain.step() {
                pages.push(page.as_bytes().to_vec());
            }
            let context = leaf
                .context
                .frames()
                .iter()
                .map(|frame| match *frame {
                    flark_integrated_parser_slice::block::BlockContainer::BlockQuote => [0, 0, 0],
                    flark_integrated_parser_slice::block::BlockContainer::BulletItem {
                        marker,
                        continuation_indent,
                    } => [1, marker, continuation_indent],
                })
                .collect();
            DirectLeaf {
                physical_start: u64::try_from(leaf.physical_start).unwrap(),
                physical_end: u64::try_from(leaf.physical_end).unwrap(),
                context,
                span_count,
                payload_bytes,
                digest,
                pages,
            }
        })
        .collect()
}

fn assert_every_arena_leaf_matches_direct(
    pages: &[Vec<u8>],
    direct: &[DirectLeaf],
    summary: OwnedParseSummary,
) {
    assert_eq!(
        pages.first().map(Vec::as_slice),
        Some(OWNED_PARSE_ANCHOR),
        "activation anchor remains the first candidate page"
    );
    let mut index = 1;
    for (ordinal, expected) in direct.iter().enumerate() {
        for expected_page in &expected.pages {
            assert_eq!(
                &pages[index], expected_page,
                "leaf {ordinal} canonical page"
            );
            index += 1;
        }
        let record = decode_leaf_record(&pages[index]).expect("leaf page has record format");
        assert_eq!(record.ordinal, u64::try_from(ordinal).unwrap());
        assert_eq!(record.physical_start, expected.physical_start);
        assert_eq!(record.physical_end, expected.physical_end);
        assert_eq!(record.span_count, expected.span_count);
        assert_eq!(
            record.canonical_page_count,
            u64::try_from(expected.pages.len()).unwrap()
        );
        assert_eq!(record.canonical_payload_bytes, expected.payload_bytes);
        assert_eq!(record.inline_digest, expected.digest);
        assert_eq!(
            &record.context[..usize::from(record.context_depth)],
            expected.context.as_slice()
        );
        index += 1;
    }
    let manifest = decode_manifest(&pages[index]).expect("root page has manifest format");
    assert_eq!(manifest.leaf_count, summary.leaf_count);
    assert_eq!(manifest.span_count, summary.span_count);
    assert_eq!(manifest.canonical_page_count, summary.canonical_page_count);
    assert_eq!(
        manifest.canonical_payload_bytes,
        summary.canonical_payload_bytes
    );
    assert_eq!(manifest.record_page_count, summary.record_page_count);
    assert_eq!(manifest.visible_pages, summary.visible_pages());
    assert_eq!(manifest.semantic_digest, summary.semantic_digest);
    assert_eq!(index + 1, pages.len(), "every physical page was classified");
}

#[test]
fn every_leaf_and_actual_canonical_page_reaches_the_owned_arena() {
    let dense = "*a* ".repeat(1_200);
    let text = format!(
        "plain `code`\n\n> quoted *em*\n> continued `code`\n\n- item **strong**\n\n{dense}"
    );
    let source = Arc::new(PersistentSource::from_text(&text));
    let direct = direct_leaves(source.clone());
    assert!(direct.len() >= 4);
    assert!(direct.iter().any(|leaf| leaf.pages.len() > 1));
    let completed = run_owned(source);
    let pages = root_chain(&completed.lifetime, completed.root);
    assert_every_arena_leaf_matches_direct(&pages, &direct, completed.summary);
    assert_eq!(
        completed.telemetry.canonical_pages_adopted,
        completed.summary.canonical_page_count
    );
    assert_eq!(
        completed.telemetry.canonical_payload_bytes_adopted,
        completed.summary.canonical_payload_bytes
    );
    assert_eq!(
        completed.telemetry.copied_record_pages,
        completed.summary.record_page_count
    );
    assert!(completed.telemetry.inline_copy_bytes > 0);
    assert!(completed.telemetry.inline_hash_bytes > 0);
}

fn assert_edit_matches_clean(base: &str, range: std::ops::Range<usize>, replacement: &str) {
    let base = PersistentSource::from_text(base);
    let edited = Arc::new(base.edit(range, replacement).unwrap().source);
    let clean = Arc::new(PersistentSource::from_text(&edited.materialize()));
    let edited_result = run_owned(edited);
    let clean_result = run_owned(clean);
    assert_eq!(
        edited_result.summary.leaf_count,
        clean_result.summary.leaf_count
    );
    assert_eq!(
        edited_result.summary.span_count,
        clean_result.summary.span_count
    );
    assert_eq!(
        edited_result.summary.canonical_page_count,
        clean_result.summary.canonical_page_count
    );
    assert_eq!(
        edited_result.summary.canonical_payload_bytes,
        clean_result.summary.canonical_payload_bytes
    );
    assert_eq!(
        edited_result.summary.semantic_digest,
        clean_result.summary.semantic_digest
    );
}

#[test]
fn prefix_insertion_and_suffix_mutation_equal_clean_supported_subset_parses() {
    let base = "first *one*\n\n> quoted `code`\n\n- last **two**";
    assert_edit_matches_clean(base, 0..0, "intro _zero_\n\n");
    let suffix = base.find("two").unwrap();
    assert_edit_matches_clean(base, suffix..suffix + 3, "three");
}

#[test]
fn many_leaves_stream_without_a_document_sized_leaf_vector() {
    let text = (0..512)
        .map(|index| format!("leaf {index} *x*"))
        .collect::<Vec<_>>()
        .join("\n\n");
    let completed = run_owned(Arc::new(PersistentSource::from_text(&text)));
    assert_eq!(completed.summary.leaf_count, 512);
    assert_eq!(completed.summary.span_count, 512);
    assert!(completed.telemetry.block_leaf_steps > completed.summary.leaf_count);
    assert!(completed.lifetime.arena_metrics().live_nodes >= 514);
}

#[test]
fn giant_plain_leaf_remains_scalar_and_has_no_canonical_payload_pages() {
    let text = "plain text ".repeat(16 * 1024);
    assert!(text.len() > 128 * 1024);
    let started = Instant::now();
    let completed = run_owned(Arc::new(PersistentSource::from_text(&text)));
    let elapsed = started.elapsed();
    assert_eq!(completed.summary.leaf_count, 1);
    assert_eq!(completed.summary.span_count, 0);
    assert_eq!(completed.summary.canonical_page_count, 0);
    assert_eq!(completed.summary.canonical_payload_bytes, 0);
    assert!(completed.slices > text.len() * 2);
    eprintln!(
        "giant leaf: {} bytes, {} scalar slices, {elapsed:?}",
        text.len(),
        completed.slices
    );
}

fn reclaim_all(scheduler: &mut Scheduler, lifetime: &mut PhysicalLifetime) -> usize {
    let mut slices = 0;
    while scheduler.pending_reclaim_tickets() != 0 {
        let report = scheduler.run_reclaim_slice(lifetime).unwrap();
        assert!(report.audit.reclaimed_nodes <= 31);
        assert!(report.audit.transitions <= 31);
        slices += 1;
    }
    slices
}

struct CancellationFixture {
    lifetime: PhysicalLifetime,
    scheduler: Scheduler,
    first_token: ParseToken,
    old_arena_job: ArenaJobId,
    first_job: OwnedParseJob,
    second_source: Arc<PersistentSource>,
}

fn cancellation_fixture() -> CancellationFixture {
    // Keep thousands of unvisited block leaves behind the dense first leaf so
    // cancellation measures both inline scratch and the still-local Arc tree.
    let trailing = (0..8_192)
        .map(|index| format!("tail {index}"))
        .collect::<Vec<_>>()
        .join("\n\n");
    let first_text = format!("{}\n\n{trailing}", "*a* ".repeat(8_000));
    let first_source = Arc::new(PersistentSource::from_text(&first_text));
    let second_source = Arc::new(PersistentSource::from_text("latest **wins**"));
    let (mut lifetime, mut scheduler) = initialize();
    let first = scheduler
        .submit_measured_source_operation(next_operation(&scheduler, &first_source))
        .unwrap();
    let ActivationSliceReport::Activated(first_activation) =
        run_measured_activation(&mut scheduler, &mut lifetime, OWNED_PARSE_ANCHOR).unwrap()
    else {
        panic!()
    };
    let mut first_job = OwnedParseJob::new(first.token, first_activation.job, first_source);
    for _ in 0..5_000_000 {
        let report = run_measured_slice(&mut scheduler, &mut lifetime, &mut first_job).unwrap();
        if first_job.phase() == OwnedParsePhase::Inline {
            break;
        }
        assert!(!matches!(report, ExecutionSliceReport::Status(_)));
    }
    assert_eq!(first_job.phase(), OwnedParsePhase::Inline);
    let mut largest_drop = first_job.drop_audit(&lifetime);
    for _ in 0..20_000_000 {
        let current = first_job.drop_audit(&lifetime);
        if current.known_bytes_lower_bound > largest_drop.known_bytes_lower_bound {
            largest_drop = current;
        }
        if current.known_bytes_lower_bound >= 32 * 1024 {
            break;
        }
        let report = run_measured_slice(&mut scheduler, &mut lifetime, &mut first_job).unwrap();
        assert!(!matches!(report, ExecutionSliceReport::Status(_)));
        assert_eq!(first_job.phase(), OwnedParsePhase::Inline);
    }
    assert!(largest_drop.known_bytes_lower_bound >= 32 * 1024);
    CancellationFixture {
        lifetime,
        scheduler,
        first_token: first.token,
        old_arena_job: first_activation.job,
        first_job,
        second_source,
    }
}

#[test]
fn latest_wins_cancels_during_inline_rejects_stale_job_and_reclaims_boundedly() {
    let CancellationFixture {
        mut lifetime,
        mut scheduler,
        first_token,
        old_arena_job,
        mut first_job,
        second_source,
    } = cancellation_fixture();

    let second = scheduler
        .submit_measured_source_operation(next_operation(&scheduler, &second_source))
        .unwrap();
    assert_eq!(second.admission, Admission::Queued);
    let ExecutionSliceReport::Status(cancelled) =
        run_measured_slice(&mut scheduler, &mut lifetime, &mut first_job).unwrap()
    else {
        panic!("supersession must not call stale parser state")
    };
    assert_eq!(cancelled.cancelled, Some(first_token));
    assert_eq!(cancelled.promoted, Some(second.token));

    let drop_started = Instant::now();
    let drop_audit = first_job.cancel(&lifetime);
    let drop_elapsed = drop_started.elapsed();
    eprintln!(
        "inline cancellation local drop: {drop_elapsed:?}, lower bound {} bytes, {} allocations",
        drop_audit.known_bytes_lower_bound, drop_audit.known_allocations_lower_bound
    );
    assert_eq!(drop_audit.phase, OwnedParsePhase::Inline);
    assert!(drop_audit.known_bytes_lower_bound > 0);
    assert!(drop_audit.unvisited_block_leaves >= 8_000);
    assert!(drop_elapsed < Duration::from_millis(100));

    let reclaim_slices = reclaim_all(&mut scheduler, &mut lifetime);
    assert!(reclaim_slices > 0);
    assert_eq!(lifetime.active_job(), None);
    let ActivationSliceReport::Activated(second_activation) =
        run_measured_activation(&mut scheduler, &mut lifetime, OWNED_PARSE_ANCHOR).unwrap()
    else {
        panic!()
    };
    assert_ne!(second_activation.job, old_arena_job);
    assert!(matches!(
        lifetime.try_append_job_page_under_limits(
            old_arena_job,
            b"stale",
            limits(),
            limits()
        ),
        Err(LifetimeError::StaleJob {
            expected,
            provided
        }) if expected == second_activation.job && provided == old_arena_job
    ));
    assert!(lifetime
        .seal_scheduler_job(&mut scheduler, first_token, 1)
        .is_err());
    assert_eq!(lifetime.active_job(), Some(second_activation.job));

    let mut second_job = OwnedParseJob::new(second.token, second_activation.job, second_source);
    loop {
        let ExecutionSliceReport::Measured(report) =
            run_measured_slice(&mut scheduler, &mut lifetime, &mut second_job).unwrap()
        else {
            panic!()
        };
        if report.status == ParseSliceStatus::ReadyToSeal {
            break;
        }
    }
    lifetime
        .seal_scheduler_job(
            &mut scheduler,
            second.token,
            second_job.visible_pages().unwrap(),
        )
        .unwrap();
    let committed = scheduler.commit_sealed(second.token).unwrap();
    assert_eq!(committed.target_source_revision, SourceRevision(2));
    assert_eq!(
        scheduler.source_root(),
        SourceRootId(second_job.summary().unwrap().source_identity.0)
    );
    scheduler.validate_invariants().unwrap();
    assert_ne!(second_activation.job, ArenaJobId(0));
}
