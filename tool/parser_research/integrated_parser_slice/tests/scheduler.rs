use std::collections::BTreeMap;

use flark_integrated_parser_slice::scheduler::{
    AckReceipt, Admission, ArenaJobId, ArenaRootId, Audit, Error, GrammarRevision, InitialState,
    ParseGeneration, ParseSliceReport, ParseSliceStatus, ParseSpec, ParseToken, ReclaimHandle,
    ReclaimOutcome, ReclaimRequest, Reclaimer, Scheduler, SliceLimits, SourceOperation,
    SourceRevision, SourceRootId, MAX_RECLAIM_TICKETS, MAX_RETAINED_ROOTS,
};

fn limits() -> SliceLimits {
    SliceLimits {
        source_bytes: 16,
        transitions: 3,
        allocated_bytes: 128,
        copied_bytes: 8,
        hashed_bytes: 16,
        index_nodes: 4,
        reclaimed_nodes: 3,
    }
}

fn slow_spec() -> ParseSpec {
    ParseSpec {
        work_units: 4,
        unit_audit: Audit {
            source_bytes: 16,
            transitions: 2,
            allocated_bytes: 64,
            copied_bytes: 8,
            hashed_bytes: 16,
            index_nodes: 1,
            reclaimed_nodes: 0,
        },
    }
}

fn new_scheduler() -> Scheduler {
    Scheduler::new(
        limits(),
        InitialState {
            source_root: SourceRootId(0),
            arena_root: ArenaRootId(10_000),
            visible_pages: 1,
        },
    )
    .unwrap()
}

fn next_operation(scheduler: &Scheduler, result_root: u64) -> SourceOperation {
    SourceOperation {
        base_revision: scheduler.source_revision(),
        target_revision: SourceRevision(scheduler.source_revision().0 + 1),
        base_root: scheduler.source_root(),
        result_root: SourceRootId(result_root),
    }
}

#[derive(Default)]
struct FakeArena {
    remaining: BTreeMap<ReclaimHandle, u64>,
    max_nodes_per_call: u64,
}

impl FakeArena {
    fn with_max_nodes_per_call(max_nodes_per_call: u64) -> Self {
        Self {
            remaining: BTreeMap::new(),
            max_nodes_per_call,
        }
    }

    fn record_parse_work(&mut self, report: ParseSliceReport) {
        if let Some(arena_job) = report.worked_arena_job {
            *self
                .remaining
                .get_mut(&ReclaimHandle::Job(arena_job))
                .unwrap() += report.progressed_units;
        }
    }

    fn activate(&mut self, arena_job: ArenaJobId) {
        assert!(
            self.remaining
                .insert(ReclaimHandle::Job(arena_job), 0)
                .is_none(),
            "arena job reused before reclamation"
        );
    }

    fn seal(&mut self, arena_job: ArenaJobId, arena_root: ArenaRootId) {
        let nodes = self
            .remaining
            .remove(&ReclaimHandle::Job(arena_job))
            .unwrap();
        assert!(
            self.remaining
                .insert(ReclaimHandle::Root(arena_root), nodes)
                .is_none(),
            "arena root reused before reclamation"
        );
    }

    fn insert_root(&mut self, arena_root: ArenaRootId, nodes: u64) {
        self.remaining
            .insert(ReclaimHandle::Root(arena_root), nodes);
    }
}

impl Reclaimer for FakeArena {
    fn reclaim(&mut self, request: ReclaimRequest) -> ReclaimOutcome {
        let handle = request.ticket.target.handle;
        let Some(nodes) = self.remaining.get_mut(&handle) else {
            return ReclaimOutcome::Rejected;
        };
        let step = (*nodes)
            .min(self.max_nodes_per_call)
            .min(request.limits.transitions)
            .min(request.limits.reclaimed_nodes);
        *nodes -= step;
        let complete = *nodes == 0;
        if complete {
            self.remaining.remove(&handle);
        }
        ReclaimOutcome::Progress {
            audit: Audit {
                transitions: step,
                reclaimed_nodes: step,
                ..Audit::ZERO
            },
            complete,
        }
    }
}

fn arena_job_for(token: ParseToken) -> ArenaJobId {
    ArenaJobId(100_000 + token.generation().0)
}

fn arena_root_for(token: ParseToken) -> ArenaRootId {
    ArenaRootId(200_000 + token.generation().0)
}

fn activate(
    scheduler: &mut Scheduler,
    arena: &mut FakeArena,
    token: ParseToken,
    arena_job: ArenaJobId,
) {
    scheduler.activate_active(token, arena_job).unwrap();
    arena.activate(arena_job);
}

#[test]
fn six_hundred_sixty_hz_edits_keep_exact_source_and_commit_only_latest() {
    let mut scheduler = new_scheduler();
    let mut arena = FakeArena::with_max_nodes_per_call(1);
    arena.insert_root(ArenaRootId(10_000), 1);
    let mut first_token = None;

    // Each loop is one logical 60 Hz input frame. The parser needs four work
    // units but the slice budget permits only one after supersession, so no
    // intermediate generation can finish before the next edit.
    for edit in 1..=600_u64 {
        let submission = scheduler
            .submit_source_operation(next_operation(&scheduler, edit), slow_spec())
            .unwrap();
        first_token.get_or_insert(submission.token);
        assert!(scheduler.parse_slot_count() <= 2);
        if submission.admission == Admission::Active {
            activate(
                &mut scheduler,
                &mut arena,
                submission.token,
                arena_job_for(submission.token),
            );
        } else {
            assert_eq!(submission.admission, Admission::Queued);
        }

        let transition = scheduler.run_parse_slice().unwrap();
        assert!(transition.audit.fits(limits()));
        arena.record_parse_work(transition);
        if let Some(promoted) = transition.promoted {
            assert_eq!(transition.status, ParseSliceStatus::AwaitingActivation);
            assert_eq!(transition.progressed_units, 0);
            activate(
                &mut scheduler,
                &mut arena,
                promoted,
                arena_job_for(promoted),
            );
            let parse = scheduler.run_parse_slice().unwrap();
            assert!(parse.audit.fits(limits()));
            assert_eq!(parse.progressed_units, 1);
            arena.record_parse_work(parse);
        } else {
            assert_eq!(transition.progressed_units, 1);
        }
        let reclaim = scheduler.run_reclaim_slice(&mut arena).unwrap();
        assert!(reclaim.audit.fits(limits()));
        assert!(scheduler.pending_reclaim_tickets() <= MAX_RECLAIM_TICKETS);
        assert!(scheduler.parse_slot_count() <= 2);
        scheduler.validate_invariants().unwrap();

        if edit == 2 {
            assert!(matches!(
                scheduler.commit_sealed(first_token.unwrap()),
                Err(Error::StaleParseGeneration {
                    provided: ParseGeneration(1),
                    latest: ParseGeneration(2)
                })
            ));
        }
    }

    assert_eq!(scheduler.source_revision(), SourceRevision(600));
    assert_eq!(scheduler.source_root(), SourceRootId(600));
    assert_eq!(scheduler.applied_operation_count(), 600);
    assert_eq!(scheduler.grammar_revision(), GrammarRevision(0));

    loop {
        let report = scheduler.run_parse_slice().unwrap();
        arena.record_parse_work(report);
        if report.status == ParseSliceStatus::ReadyToSeal {
            break;
        }
    }
    let latest = scheduler.active_parse_token().unwrap();
    let arena_job = arena_job_for(latest);
    let arena_root = arena_root_for(latest);
    arena.seal(arena_job, arena_root);
    scheduler
        .seal_active(latest, arena_job, arena_root, 12)
        .unwrap();
    let delta = scheduler.commit_sealed(latest).unwrap();

    assert_eq!(delta.base_grammar_revision, GrammarRevision(0));
    assert_eq!(delta.target_source_revision, SourceRevision(600));
    assert_eq!(delta.source_root, SourceRootId(600));
    assert_eq!(delta.parse_generation, ParseGeneration(600));
    assert_eq!(scheduler.grammar_revision(), GrammarRevision(600));
    assert_eq!(scheduler.adopted_source_root(), SourceRootId(600));
    assert_eq!(scheduler.parse_slot_count(), 0);
    scheduler.validate_invariants().unwrap();
}

fn commit_one(
    scheduler: &mut Scheduler,
    arena: &mut FakeArena,
    source_root: u64,
    arena_root: ArenaRootId,
    visible_pages: u32,
) -> flark_integrated_parser_slice::scheduler::RootLease {
    let spec = ParseSpec {
        work_units: 1,
        unit_audit: Audit {
            source_bytes: 1,
            transitions: 1,
            allocated_bytes: 32,
            copied_bytes: 0,
            hashed_bytes: 1,
            index_nodes: 1,
            reclaimed_nodes: 0,
        },
    };
    let token = scheduler
        .submit_source_operation(next_operation(scheduler, source_root), spec)
        .unwrap()
        .token;
    let arena_job = arena_job_for(token);
    activate(scheduler, arena, token, arena_job);
    let parse = scheduler.run_parse_slice().unwrap();
    assert_eq!(parse.status, ParseSliceStatus::ReadyToSeal);
    arena.record_parse_work(parse);
    arena.seal(arena_job, arena_root);
    scheduler
        .seal_active(token, arena_job, arena_root, visible_pages)
        .unwrap();
    scheduler.commit_sealed(token).unwrap().root_lease
}

#[test]
fn stalled_ack_keeps_roots_bounded_and_old_generation_queries_go_stale() {
    let mut scheduler = new_scheduler();
    let mut arena = FakeArena::with_max_nodes_per_call(3);
    arena.insert_root(ArenaRootId(10_000), 1);
    let initial_ack = scheduler.acknowledged_root_lease().unwrap();
    let mut first_offer = None;
    let mut latest_offer = None;

    for revision in 1..=40_u64 {
        let lease = commit_one(
            &mut scheduler,
            &mut arena,
            revision,
            ArenaRootId(30_000 + revision),
            10,
        );
        first_offer.get_or_insert(lease);
        latest_offer = Some(lease);
        assert!(scheduler.retained_root_count() <= 2);
        assert!(scheduler.retained_root_count() <= MAX_RETAINED_ROOTS);
        scheduler.validate_invariants().unwrap();
        scheduler.run_reclaim_slice(&mut arena).unwrap();
    }

    let first_offer = first_offer.unwrap();
    let latest_offer = latest_offer.unwrap();
    assert!(matches!(
        scheduler.query_visible_pages(first_offer, 0, 1),
        Err(Error::StaleRootLease { .. })
    ));

    let page_batch = scheduler.query_visible_pages(latest_offer, 2, 4).unwrap();
    assert_eq!(page_batch.pages.len(), 3);
    assert_eq!(page_batch.pages[0].page_index, 2);
    assert_eq!(page_batch.next_page, Some(5));
    assert!(page_batch.audit.fits(limits()));

    assert_eq!(
        scheduler.acknowledge_root(latest_offer).unwrap(),
        AckReceipt {
            acknowledged: latest_offer,
            released_previous: Some(initial_ack),
        }
    );
    assert_eq!(scheduler.retained_root_count(), 1);
    assert!(scheduler.query_visible_pages(latest_offer, 0, 2).is_ok());

    scheduler.release_root(latest_offer).unwrap();
    assert!(matches!(
        scheduler.query_visible_pages(latest_offer, 0, 1),
        Err(Error::StaleRootLease { .. })
    ));
    // The released root is still the worker's adopted G, but is no longer a
    // remotely live lease.
    assert_eq!(scheduler.retained_root_count(), 1);
    scheduler.validate_invariants().unwrap();
}

#[test]
fn cancellation_cleanup_backpressures_at_fixed_capacity_and_reclaims_in_slices() {
    let mut scheduler = new_scheduler();
    let mut arena = FakeArena::with_max_nodes_per_call(1);
    arena.insert_root(ArenaRootId(10_000), 1);
    let spec = ParseSpec {
        work_units: 20,
        unit_audit: Audit {
            source_bytes: 1,
            transitions: 1,
            allocated_bytes: 32,
            copied_bytes: 1,
            hashed_bytes: 1,
            index_nodes: 1,
            reclaimed_nodes: 0,
        },
    };

    for revision in 1..=u64::try_from(MAX_RECLAIM_TICKETS + 1).unwrap() {
        let submission = scheduler
            .submit_source_operation(next_operation(&scheduler, 100 + revision), spec)
            .unwrap();
        if submission.admission == Admission::Active {
            activate(
                &mut scheduler,
                &mut arena,
                submission.token,
                arena_job_for(submission.token),
            );
        }
        let report = scheduler.run_parse_slice().unwrap();
        arena.record_parse_work(report);
        if let Some(promoted) = report.promoted {
            assert_eq!(report.status, ParseSliceStatus::AwaitingActivation);
            activate(
                &mut scheduler,
                &mut arena,
                promoted,
                arena_job_for(promoted),
            );
            let work = scheduler.run_parse_slice().unwrap();
            arena.record_parse_work(work);
        }
        assert!(scheduler.pending_reclaim_tickets() <= MAX_RECLAIM_TICKETS);
    }
    assert_eq!(scheduler.pending_reclaim_tickets(), MAX_RECLAIM_TICKETS);

    // Source S continues to advance and only the one queued descriptor changes
    // while parser cancellation is backpressured.
    for revision in 20..40_u64 {
        scheduler
            .submit_source_operation(next_operation(&scheduler, 100 + revision), spec)
            .unwrap();
        let report = scheduler.run_parse_slice().unwrap();
        assert_eq!(report.status, ParseSliceStatus::ReclaimBackpressure);
        assert_eq!(report.audit, Audit::ZERO);
        assert_eq!(scheduler.pending_reclaim_tickets(), MAX_RECLAIM_TICKETS);
        assert!(scheduler.parse_slot_count() <= 2);
    }

    let mut reclaim_slices = 0;
    while scheduler.pending_reclaim_tickets() > 0 {
        let report = scheduler.run_reclaim_slice(&mut arena).unwrap();
        assert!(report.audit.fits(limits()));
        assert!(report.audit.reclaimed_nodes <= limits().reclaimed_nodes);
        assert!(report.audit.transitions <= limits().transitions);
        reclaim_slices += 1;
    }
    assert!(reclaim_slices >= 2);

    let promoted = scheduler.run_parse_slice().unwrap();
    assert_eq!(promoted.status, ParseSliceStatus::AwaitingActivation);
    assert!(promoted.cancelled.is_some());
    assert_eq!(promoted.promoted, scheduler.active_parse_token());
    let promoted_token = promoted.promoted.unwrap();
    activate(
        &mut scheduler,
        &mut arena,
        promoted_token,
        arena_job_for(promoted_token),
    );
    let work = scheduler.run_parse_slice().unwrap();
    arena.record_parse_work(work);
    scheduler.validate_invariants().unwrap();
}

#[test]
fn sealing_atomically_substitutes_job_with_root_for_later_cancellation() {
    let mut scheduler = new_scheduler();
    let mut arena = FakeArena::with_max_nodes_per_call(3);
    arena.insert_root(ArenaRootId(10_000), 1);
    let spec = ParseSpec {
        work_units: 1,
        unit_audit: Audit {
            source_bytes: 1,
            transitions: 1,
            allocated_bytes: 32,
            copied_bytes: 0,
            hashed_bytes: 1,
            index_nodes: 1,
            reclaimed_nodes: 0,
        },
    };
    let first = scheduler
        .submit_source_operation(next_operation(&scheduler, 1), spec)
        .unwrap()
        .token;
    let job = arena_job_for(first);
    let root = arena_root_for(first);
    activate(&mut scheduler, &mut arena, first, job);
    let work = scheduler.run_parse_slice().unwrap();
    arena.record_parse_work(work);
    assert!(matches!(
        scheduler.seal_active(first, ArenaJobId(job.0 + 1), root, 2),
        Err(Error::ArenaJobMismatch { .. })
    ));
    arena.seal(job, root);
    scheduler.seal_active(first, job, root, 2).unwrap();
    assert!(!arena.remaining.contains_key(&ReclaimHandle::Job(job)));
    assert!(arena.remaining.contains_key(&ReclaimHandle::Root(root)));

    scheduler
        .submit_source_operation(next_operation(&scheduler, 2), spec)
        .unwrap();
    let supersession = scheduler.run_parse_slice().unwrap();
    assert_eq!(supersession.cancelled, Some(first));
    assert_eq!(scheduler.pending_reclaim_tickets(), 1);
    let reclaimed = scheduler.run_reclaim_slice(&mut arena).unwrap();
    assert_eq!(reclaimed.completed_tickets, 1);
    assert!(!arena.remaining.contains_key(&ReclaimHandle::Root(root)));
    scheduler.validate_invariants().unwrap();
}

#[test]
fn every_audit_dimension_is_enforced_before_source_state_changes() {
    let mut scheduler = new_scheduler();
    let too_expensive = ParseSpec {
        work_units: 1,
        unit_audit: Audit {
            source_bytes: limits().source_bytes + 1,
            transitions: limits().transitions + 1,
            allocated_bytes: limits().allocated_bytes + 1,
            copied_bytes: limits().copied_bytes + 1,
            hashed_bytes: limits().hashed_bytes + 1,
            index_nodes: limits().index_nodes + 1,
            reclaimed_nodes: 0,
        },
    };
    assert!(matches!(
        scheduler.submit_source_operation(next_operation(&scheduler, 1), too_expensive),
        Err(Error::ParseUnitExceedsSliceLimits { .. })
    ));
    assert_eq!(scheduler.source_revision(), SourceRevision(0));
    assert_eq!(scheduler.source_root(), SourceRootId(0));
    assert_eq!(scheduler.applied_operation_count(), 0);
    assert_eq!(scheduler.parse_slot_count(), 0);

    let valid = ParseSpec {
        work_units: 1,
        unit_audit: Audit {
            source_bytes: 1,
            transitions: 1,
            allocated_bytes: 1,
            copied_bytes: 1,
            hashed_bytes: 1,
            index_nodes: 1,
            reclaimed_nodes: 0,
        },
    };
    let first = scheduler
        .submit_source_operation(next_operation(&scheduler, 1), valid)
        .unwrap()
        .token;
    scheduler
        .activate_active(first, ArenaJobId(900_001))
        .unwrap();
    scheduler.run_parse_slice().unwrap();
    scheduler
        .submit_source_operation(next_operation(&scheduler, 2), valid)
        .unwrap();
    scheduler.run_parse_slice().unwrap();
    assert_eq!(scheduler.pending_reclaim_tickets(), 1);

    let base = Audit {
        transitions: 1,
        reclaimed_nodes: 1,
        ..Audit::ZERO
    };
    let over_budget_receipts = [
        Audit {
            source_bytes: limits().source_bytes + 1,
            ..base
        },
        Audit {
            transitions: limits().transitions + 1,
            ..base
        },
        Audit {
            allocated_bytes: limits().allocated_bytes + 1,
            ..base
        },
        Audit {
            copied_bytes: limits().copied_bytes + 1,
            ..base
        },
        Audit {
            hashed_bytes: limits().hashed_bytes + 1,
            ..base
        },
        Audit {
            index_nodes: limits().index_nodes + 1,
            ..base
        },
        Audit {
            reclaimed_nodes: limits().reclaimed_nodes + 1,
            ..base
        },
    ];
    for audit in over_budget_receipts {
        let mut reclaimer = ReceiptReclaimer(audit);
        assert!(matches!(
            scheduler.run_reclaim_slice(&mut reclaimer),
            Err(Error::ReclaimReceiptExceedsSliceLimits { .. })
        ));
        assert_eq!(scheduler.pending_reclaim_tickets(), 1);
    }
    scheduler.validate_invariants().unwrap();
}

struct ReceiptReclaimer(Audit);

impl Reclaimer for ReceiptReclaimer {
    fn reclaim(&mut self, _request: ReclaimRequest) -> ReclaimOutcome {
        ReclaimOutcome::Progress {
            audit: self.0,
            complete: true,
        }
    }
}
