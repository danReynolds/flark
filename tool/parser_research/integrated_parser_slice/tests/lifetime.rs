use flark_integrated_parser_slice::lifetime::{LifetimeError, PhysicalLifetime};
use flark_integrated_parser_slice::scheduler::{
    Admission, ArenaJobId, ArenaRootId, Audit, ParseSliceStatus, ParseSpec, ReclaimHandle,
    ReclaimOutcome, ReclaimReason, ReclaimRequest, ReclaimTarget, ReclaimTicket, Reclaimer,
    Scheduler, SliceLimits, SourceOperation, SourceRevision, SourceRootId, MAX_RECLAIM_TICKETS,
    MAX_RETAINED_ROOTS,
};

fn limits(transitions: u64) -> SliceLimits {
    SliceLimits {
        source_bytes: u64::MAX,
        transitions,
        allocated_bytes: u64::MAX,
        copied_bytes: u64::MAX,
        hashed_bytes: u64::MAX,
        index_nodes: u64::MAX,
        reclaimed_nodes: transitions,
    }
}

fn spec(work_units: u64) -> ParseSpec {
    ParseSpec {
        work_units,
        unit_audit: Audit {
            source_bytes: 1,
            transitions: 1,
            allocated_bytes: 1,
            copied_bytes: 1,
            hashed_bytes: 1,
            index_nodes: 1,
            reclaimed_nodes: 0,
        },
    }
}

fn new_document(transitions: u64) -> (PhysicalLifetime, Scheduler, ArenaRootId) {
    let mut lifetime = PhysicalLifetime::new();
    let (scheduler, root, receipt) = lifetime
        .initialize_scheduler(limits(transitions), SourceRootId(1), 1, b"initial root")
        .unwrap();
    assert_eq!(receipt.allocation.id.generation, 1);
    assert_eq!(lifetime.root_payload(root).unwrap(), b"initial root");
    (lifetime, scheduler, root)
}

fn next_operation(scheduler: &Scheduler, root: u64) -> SourceOperation {
    SourceOperation {
        base_revision: scheduler.source_revision(),
        target_revision: SourceRevision(scheduler.source_revision().0 + 1),
        base_root: scheduler.source_root(),
        result_root: SourceRootId(root),
    }
}

fn reclaim_all(scheduler: &mut Scheduler, lifetime: &mut PhysicalLifetime) -> usize {
    let mut slices = 0;
    while scheduler.pending_reclaim_tickets() > 0 {
        let report = scheduler.run_reclaim_slice(lifetime).unwrap();
        assert!(report.audit.fits(scheduler.slice_limits()));
        assert!(report.audit.transitions <= scheduler.slice_limits().transitions);
        assert!(report.audit.reclaimed_nodes <= scheduler.slice_limits().reclaimed_nodes);
        slices += 1;
    }
    slices
}

#[test]
fn cancellation_before_seal_reclaims_the_actual_job_graph() {
    let (mut lifetime, mut scheduler, initial_root) = new_document(2);
    let first = scheduler
        .submit_source_operation(next_operation(&scheduler, 2), spec(4))
        .unwrap();
    assert_eq!(first.admission, Admission::Active);
    let activation = lifetime
        .activate_scheduler_job(&mut scheduler, first.token, b"job anchor")
        .unwrap();
    let parse = scheduler.run_parse_slice().unwrap();
    assert_eq!(parse.progressed_units, 2);
    for ordinal in 0..parse.progressed_units {
        lifetime
            .append_job_page(activation.job, &[u8::try_from(ordinal).unwrap()])
            .unwrap();
    }
    assert_eq!(lifetime.active_job_pages(), 3);
    assert_eq!(lifetime.arena_metrics().live_nodes, 4);

    let second = scheduler
        .submit_source_operation(next_operation(&scheduler, 3), spec(1))
        .unwrap();
    assert_eq!(second.admission, Admission::Queued);
    let supersession = scheduler.run_parse_slice().unwrap();
    assert_eq!(supersession.status, ParseSliceStatus::AwaitingActivation);
    assert_eq!(supersession.cancelled, Some(first.token));
    assert_eq!(supersession.promoted, Some(second.token));
    assert!(matches!(
        lifetime.activate_scheduler_job(&mut scheduler, second.token, b"too early"),
        Err(LifetimeError::ActiveJobExists(job)) if job == activation.job
    ));
    assert_eq!(scheduler.pending_reclaim_tickets(), 1);

    let slices = reclaim_all(&mut scheduler, &mut lifetime);
    assert_eq!(slices, 2);
    assert_eq!(lifetime.active_job(), None);
    assert_eq!(lifetime.arena_metrics().live_nodes, 1);
    assert_eq!(
        lifetime.root_payload(initial_root).unwrap(),
        b"initial root"
    );

    let promoted = lifetime
        .activate_scheduler_job(&mut scheduler, second.token, b"promoted")
        .unwrap();
    assert_ne!(promoted.job, activation.job);
    scheduler.validate_invariants().unwrap();
}

#[test]
fn supersession_after_seal_reclaims_the_root_namespace_not_the_job_namespace() {
    let (mut lifetime, mut scheduler, _) = new_document(1);
    let first = scheduler
        .submit_source_operation(next_operation(&scheduler, 2), spec(1))
        .unwrap();
    let activation = lifetime
        .activate_scheduler_job(&mut scheduler, first.token, b"candidate")
        .unwrap();
    assert_eq!(
        scheduler.run_parse_slice().unwrap().status,
        ParseSliceStatus::ReadyToSeal
    );
    lifetime
        .append_job_page(activation.job, b"sealed page")
        .unwrap();
    let sealed_root = lifetime
        .seal_scheduler_job(&mut scheduler, first.token, 2)
        .unwrap();
    assert_eq!(lifetime.active_job(), None);
    assert_eq!(lifetime.root_payload(sealed_root).unwrap(), b"sealed page");

    let second = scheduler
        .submit_source_operation(next_operation(&scheduler, 3), spec(1))
        .unwrap();
    let supersession = scheduler.run_parse_slice().unwrap();
    assert_eq!(supersession.promoted, Some(second.token));
    assert_eq!(scheduler.pending_reclaim_tickets(), 1);
    assert_eq!(lifetime.in_flight_target(), None);

    let promoted = lifetime
        .activate_scheduler_job(&mut scheduler, second.token, b"new candidate")
        .unwrap();
    assert_eq!(
        scheduler.run_parse_slice().unwrap().status,
        ParseSliceStatus::ReadyToSeal
    );
    let first_reclaim = scheduler.run_reclaim_slice(&mut lifetime).unwrap();
    assert_eq!(first_reclaim.audit.reclaimed_nodes, 1);
    assert_eq!(first_reclaim.remaining_tickets, 1);
    let pending_before_append = lifetime.arena_metrics().pending_releases;
    let appended = lifetime
        .append_job_page(promoted.job, b"new page during old reclaim")
        .unwrap();
    assert_eq!(appended.audit().transitions, 2);
    assert_eq!(
        lifetime.arena_metrics().pending_releases,
        pending_before_append,
        "atomic child transfer must not enter the old ticket's FIFO"
    );
    reclaim_all(&mut scheduler, &mut lifetime);
    assert!(lifetime.root_payload(sealed_root).is_err());
    assert_eq!(lifetime.arena_metrics().live_nodes, 3);
    scheduler.validate_invariants().unwrap();
}

fn commit_current(
    lifetime: &mut PhysicalLifetime,
    scheduler: &mut Scheduler,
    source_root: u64,
) -> (
    flark_integrated_parser_slice::scheduler::RootLease,
    ArenaRootId,
) {
    let submission = scheduler
        .submit_source_operation(next_operation(scheduler, source_root), spec(1))
        .unwrap();
    let activation = lifetime
        .activate_scheduler_job(scheduler, submission.token, b"root")
        .unwrap();
    let report = scheduler.run_parse_slice().unwrap();
    assert_eq!(report.status, ParseSliceStatus::ReadyToSeal);
    lifetime
        .append_job_page(activation.job, &source_root.to_le_bytes())
        .unwrap();
    let root = lifetime
        .seal_scheduler_job(scheduler, submission.token, 1)
        .unwrap();
    let delta = scheduler.commit_sealed(submission.token).unwrap();
    (delta.root_lease, root)
}

#[test]
fn remote_root_retirement_frees_real_pages_and_never_exceeds_three_roots() {
    let (mut lifetime, mut scheduler, initial_root) = new_document(8);
    let (first_lease, first_root) = commit_current(&mut lifetime, &mut scheduler, 2);
    assert!(scheduler.retained_root_count() <= MAX_RETAINED_ROOTS);
    scheduler.acknowledge_root(first_lease).unwrap();
    assert_eq!(scheduler.pending_reclaim_tickets(), 1);
    reclaim_all(&mut scheduler, &mut lifetime);
    assert!(lifetime.root_payload(initial_root).is_err());

    let (second_lease, second_root) = commit_current(&mut lifetime, &mut scheduler, 3);
    assert!(scheduler.retained_root_count() <= MAX_RETAINED_ROOTS);
    scheduler.acknowledge_root(second_lease).unwrap();
    assert_eq!(scheduler.pending_reclaim_tickets(), 1);
    reclaim_all(&mut scheduler, &mut lifetime);
    assert!(lifetime.root_payload(first_root).is_err());
    assert_eq!(
        lifetime.root_payload(second_root).unwrap(),
        3_u64.to_le_bytes()
    );
    assert_eq!(scheduler.retained_root_count(), 1);
    scheduler.validate_invariants().unwrap();
}

#[test]
fn deep_tree_reclaim_is_iterative_and_scheduler_fuelled() {
    const PAGES: u64 = 4_096;
    let (mut lifetime, mut scheduler, _) = new_document(7);
    let first = scheduler
        .submit_source_operation(next_operation(&scheduler, 2), spec(PAGES))
        .unwrap();
    let activation = lifetime
        .activate_scheduler_job(&mut scheduler, first.token, b"anchor")
        .unwrap();
    let mut appended = 0_u64;
    loop {
        let report = scheduler.run_parse_slice().unwrap();
        for _ in 0..report.progressed_units {
            lifetime
                .append_job_page(activation.job, &appended.to_le_bytes())
                .unwrap();
            appended += 1;
        }
        if report.status == ParseSliceStatus::ReadyToSeal {
            break;
        }
    }
    assert_eq!(appended, PAGES);
    assert_eq!(
        lifetime.active_job_pages(),
        usize::try_from(PAGES).unwrap() + 1
    );
    assert_eq!(
        lifetime.arena_metrics().live_nodes,
        usize::try_from(PAGES).unwrap() + 2
    );

    let second = scheduler
        .submit_source_operation(next_operation(&scheduler, 3), spec(1))
        .unwrap();
    assert_eq!(
        scheduler.run_parse_slice().unwrap().promoted,
        Some(second.token)
    );
    let first_reclaim = scheduler.run_reclaim_slice(&mut lifetime).unwrap();
    assert_eq!(first_reclaim.completed_tickets, 0);
    assert_eq!(lifetime.active_job(), None);
    assert!(lifetime.arena_metrics().pending_releases > 0);

    let before_failed_activation = lifetime.arena_metrics();
    assert!(matches!(
        lifetime.activate_scheduler_job(&mut scheduler, first.token, b"stale anchor"),
        Err(LifetimeError::Scheduler(_))
    ));
    assert_eq!(lifetime.arena_metrics(), before_failed_activation);

    let latest = lifetime
        .activate_scheduler_job(&mut scheduler, second.token, b"latest anchor")
        .unwrap();
    lifetime
        .append_job_page(latest.job, b"latest page")
        .unwrap();
    assert!(lifetime.arena_metrics().pending_releases > 0);

    let slices = 1 + reclaim_all(&mut scheduler, &mut lifetime);
    assert!(slices > 500);
    // Initial committed root plus the latest active two-page job survive.
    assert_eq!(lifetime.arena_metrics().live_nodes, 3);
    assert_eq!(lifetime.arena_metrics().pending_releases, 0);
}

#[test]
fn opaque_handles_reject_wrong_jobs_malformed_roots_and_stale_releases() {
    let (mut lifetime, mut scheduler, initial_root) = new_document(4);
    assert!(matches!(
        lifetime.root_payload(ArenaRootId(0)),
        Err(LifetimeError::InvalidRootHandle(ArenaRootId(0)))
    ));
    let submission = scheduler
        .submit_source_operation(next_operation(&scheduler, 2), spec(1))
        .unwrap();
    let activation = lifetime
        .activate_scheduler_job(&mut scheduler, submission.token, b"job")
        .unwrap();
    assert!(matches!(
        lifetime.append_job_page(ArenaJobId(activation.job.0 + 1), b"wrong"),
        Err(LifetimeError::StaleJob { .. })
    ));

    let queued = scheduler
        .submit_source_operation(next_operation(&scheduler, 3), spec(1))
        .unwrap();
    assert_eq!(
        scheduler.run_parse_slice().unwrap().promoted,
        Some(queued.token)
    );
    reclaim_all(&mut scheduler, &mut lifetime);
    let stale_job_request = ReclaimRequest {
        ticket: ReclaimTicket {
            target: ReclaimTarget {
                handle: ReclaimHandle::Job(activation.job),
                reason: ReclaimReason::CandidateCancelled,
            },
        },
        limits: limits(1),
    };
    assert_eq!(
        lifetime.reclaim(stale_job_request),
        ReclaimOutcome::Rejected
    );

    let promoted = lifetime
        .activate_scheduler_job(&mut scheduler, queued.token, b"replacement")
        .unwrap();
    assert_eq!(
        scheduler.run_parse_slice().unwrap().status,
        ParseSliceStatus::ReadyToSeal
    );
    lifetime
        .append_job_page(promoted.job, b"replacement root")
        .unwrap();
    lifetime
        .seal_scheduler_job(&mut scheduler, queued.token, 1)
        .unwrap();
    let lease = scheduler.commit_sealed(queued.token).unwrap().root_lease;
    scheduler.acknowledge_root(lease).unwrap();
    reclaim_all(&mut scheduler, &mut lifetime);
    assert!(lifetime.root_payload(initial_root).is_err());

    let root_request = ReclaimRequest {
        ticket: ReclaimTicket {
            target: ReclaimTarget {
                handle: ReclaimHandle::Root(initial_root),
                reason: ReclaimReason::RemoteRootRetired,
            },
        },
        limits: limits(4),
    };
    assert_eq!(lifetime.reclaim(root_request), ReclaimOutcome::Rejected);
}

#[test]
fn scheduler_queue_is_capped_at_four_while_lifetime_keeps_one_active_slot() {
    let (mut lifetime, mut scheduler, _) = new_document(8);
    let mut current = scheduler
        .submit_source_operation(next_operation(&scheduler, 2), spec(1))
        .unwrap()
        .token;

    for source_root in 3..=6_u64 {
        let activation = lifetime
            .activate_scheduler_job(&mut scheduler, current, b"candidate")
            .unwrap();
        assert_eq!(lifetime.active_job(), Some(activation.job));
        assert_eq!(
            scheduler.run_parse_slice().unwrap().status,
            ParseSliceStatus::ReadyToSeal
        );
        lifetime
            .seal_scheduler_job(&mut scheduler, current, 1)
            .unwrap();
        let next = scheduler
            .submit_source_operation(next_operation(&scheduler, source_root), spec(1))
            .unwrap();
        assert_eq!(next.admission, Admission::Queued);
        let report = scheduler.run_parse_slice().unwrap();
        assert_eq!(report.promoted, Some(next.token));
        current = next.token;
        assert!(scheduler.pending_reclaim_tickets() <= MAX_RECLAIM_TICKETS);
        assert_eq!(lifetime.active_job(), None);
    }
    assert_eq!(scheduler.pending_reclaim_tickets(), MAX_RECLAIM_TICKETS);

    let activation = lifetime
        .activate_scheduler_job(&mut scheduler, current, b"backpressured")
        .unwrap();
    assert_eq!(
        scheduler.run_parse_slice().unwrap().status,
        ParseSliceStatus::ReadyToSeal
    );
    lifetime
        .seal_scheduler_job(&mut scheduler, current, 1)
        .unwrap();
    let queued = scheduler
        .submit_source_operation(next_operation(&scheduler, 7), spec(1))
        .unwrap();
    let report = scheduler.run_parse_slice().unwrap();
    assert_eq!(report.status, ParseSliceStatus::ReclaimBackpressure);
    assert_eq!(report.promoted, None);
    assert_eq!(scheduler.queued_parse_token(), Some(queued.token));
    assert_eq!(scheduler.pending_reclaim_tickets(), MAX_RECLAIM_TICKETS);
    assert!(scheduler.retained_root_count() <= MAX_RETAINED_ROOTS);
    assert_ne!(activation.job, ArenaJobId(0));

    let first_reclaim = scheduler.run_reclaim_slice(&mut lifetime).unwrap();
    assert!(first_reclaim.completed_tickets > 0);
    assert!(scheduler.pending_reclaim_tickets() < MAX_RECLAIM_TICKETS);
    let promoted = scheduler.run_parse_slice().unwrap();
    assert_eq!(promoted.promoted, Some(queued.token));
    assert!(scheduler.pending_reclaim_tickets() <= MAX_RECLAIM_TICKETS);
    reclaim_all(&mut scheduler, &mut lifetime);
    scheduler.validate_invariants().unwrap();
}
