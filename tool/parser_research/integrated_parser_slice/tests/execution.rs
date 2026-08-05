use flark_integrated_parser_slice::execution::{
    run_measured_activation, run_measured_slice, ActivationSliceReport, ExecutionError,
    ExecutionSliceReport, GrammarExecutionGap, JobPollFailure, MeasuredParseJob,
    MeteredDelimiterPageJob, EXECUTION_PROTOCOL_GAPS, GRAMMAR_EXECUTION_GAPS,
};
use flark_integrated_parser_slice::lifetime::{
    JobActivationReceipt, LifetimeError, PhysicalLifetime,
};
use flark_integrated_parser_slice::scheduler::{
    ArenaJobId, Audit, Error, GrammarRevision, MeasuredParseReceipt, ParseSliceStatus, ParseSpec,
    ParseToken, ParseWorkAvailability, ParseWorkPermit, Scheduler, SliceLimits, SourceOperation,
    SourceRevision, SourceRootId,
};
use std::fmt;

fn limits(copied_bytes: u64) -> SliceLimits {
    SliceLimits {
        // This is the byte-work fuel: at most one input byte per slice.
        source_bytes: 1,
        // A final slice may inspect one byte and append one parent page (the
        // allocation receipt charges its transition plus child transfer).
        transitions: 3,
        allocated_bytes: 64 * 1024,
        copied_bytes,
        hashed_bytes: 1,
        index_nodes: 8,
        reclaimed_nodes: 3,
    }
}

fn next_operation(scheduler: &Scheduler, result_root: u64) -> SourceOperation {
    SourceOperation {
        base_revision: scheduler.source_revision(),
        target_revision: SourceRevision(scheduler.source_revision().0 + 1),
        base_root: scheduler.source_root(),
        result_root: SourceRootId(result_root),
    }
}

fn initialize(slice_limits: SliceLimits) -> (PhysicalLifetime, Scheduler) {
    let mut lifetime = PhysicalLifetime::new();
    let (scheduler, _, _) = lifetime
        .initialize_scheduler(slice_limits, SourceRootId(0), 1, b"initial")
        .unwrap();
    (lifetime, scheduler)
}

fn reclaim_all(scheduler: &mut Scheduler, lifetime: &mut PhysicalLifetime) {
    while scheduler.pending_reclaim_tickets() != 0 {
        let report = scheduler.run_reclaim_slice(lifetime).unwrap();
        assert!(report.audit.fits(scheduler.slice_limits()));
    }
}

fn activate_measured(
    scheduler: &mut Scheduler,
    lifetime: &mut PhysicalLifetime,
    payload: &[u8],
) -> JobActivationReceipt {
    let ActivationSliceReport::Activated(receipt) =
        run_measured_activation(scheduler, lifetime, payload).unwrap()
    else {
        panic!("current measured generation must issue an activation capability");
    };
    assert!(receipt.audit().fits(scheduler.slice_limits()));
    receipt
}

#[test]
fn real_job_under_tiny_byte_fuel_advances_only_from_measured_receipts() {
    let (mut lifetime, mut scheduler) = initialize(limits(u64::MAX));
    let input: Box<[u8]> = Box::from(&b"a*b"[..]);
    let submission = scheduler
        .submit_measured_source_operation(next_operation(&scheduler, 1))
        .unwrap();
    let activation = activate_measured(&mut scheduler, &mut lifetime, b"");
    let mut job = MeteredDelimiterPageJob::new(submission.token, activation.job, input);

    let mut accepted_audit = Audit::ZERO;
    let mut slices = 0;
    loop {
        let ExecutionSliceReport::Measured(report) =
            run_measured_slice(&mut scheduler, &mut lifetime, &mut job).unwrap()
        else {
            panic!("activated current job must receive work");
        };
        assert!(report.audit.fits(scheduler.slice_limits()));
        assert!(report.audit.source_bytes <= 1);
        accepted_audit.source_bytes += report.audit.source_bytes;
        accepted_audit.transitions += report.audit.transitions;
        accepted_audit.allocated_bytes += report.audit.allocated_bytes;
        accepted_audit.copied_bytes += report.audit.copied_bytes;
        accepted_audit.hashed_bytes += report.audit.hashed_bytes;
        accepted_audit.index_nodes += report.audit.index_nodes;
        slices += 1;
        if report.status == ParseSliceStatus::ReadyToSeal {
            break;
        }
    }

    assert_eq!(slices, 3);
    assert_eq!(accepted_audit.source_bytes, 3);
    assert_eq!(accepted_audit.hashed_bytes, 3);
    assert_eq!(accepted_audit.copied_bytes, 16);
    assert!(accepted_audit.allocated_bytes >= 16);
    assert_eq!(lifetime.active_job_pages(), 2);
    assert_eq!(job.delimiter_count(), Some(1));

    let root = lifetime
        .seal_scheduler_job(&mut scheduler, submission.token, 1)
        .unwrap();
    let delta = scheduler.commit_sealed(submission.token).unwrap();
    assert_eq!(delta.target_source_revision, SourceRevision(1));
    assert_eq!(scheduler.grammar_revision(), GrammarRevision(1));
    let payload = lifetime.root_payload(root).unwrap();
    assert_eq!(payload.len(), 16);
    assert_eq!(u64::from_le_bytes(payload[8..].try_into().unwrap()), 1);
}

#[test]
fn physical_audit_overrun_is_rejected_without_false_progress_then_reclaimed() {
    let (mut lifetime, mut scheduler) = initialize(limits(0));
    let first = scheduler
        .submit_measured_source_operation(next_operation(&scheduler, 1))
        .unwrap();
    // The parse permit intentionally allows no copied bytes; use a zero-byte
    // anchor so activation remains legal and the injected output copy is the
    // operation that violates its own permit.
    let activation = activate_measured(&mut scheduler, &mut lifetime, b"");
    let mut job = OverrunPageJob {
        token: first.token,
        arena_job: activation.job,
        ready: false,
    };

    let error = run_measured_slice(&mut scheduler, &mut lifetime, &mut job).unwrap_err();
    assert!(matches!(
        error,
        ExecutionError::Scheduler(Error::MeasuredParseReceiptExceedsLimits {
            audit: Audit {
                copied_bytes: 16,
                ..
            },
            ..
        })
    ));
    assert_eq!(scheduler.grammar_revision(), GrammarRevision(0));
    assert!(matches!(
        lifetime.seal_scheduler_job(&mut scheduler, first.token, 1),
        Err(
            flark_integrated_parser_slice::lifetime::LifetimeError::Scheduler(
                Error::MeasuredParsePoisoned
            )
        )
    ));
    assert_eq!(
        lifetime.active_job_pages(),
        2,
        "real output allocation occurred"
    );

    let promotion = scheduler.retry_poisoned_measured_parse().unwrap();
    assert_eq!(promotion.cancelled, Some(first.token));
    assert_ne!(promotion.promoted, Some(first.token));
    assert_eq!(scheduler.pending_reclaim_tickets(), 1);
    reclaim_all(&mut scheduler, &mut lifetime);
    assert_eq!(lifetime.active_job(), None);
    assert_eq!(lifetime.arena_metrics().live_nodes, 1);
    assert_eq!(scheduler.grammar_revision(), GrammarRevision(0));
}

#[test]
fn source_supersession_invalidates_an_issued_receipt_without_adopting_it() {
    let (mut lifetime, mut scheduler) = initialize(limits(u64::MAX));
    let first = scheduler
        .submit_measured_source_operation(next_operation(&scheduler, 1))
        .unwrap();
    let activation = activate_measured(&mut scheduler, &mut lifetime, b"candidate");
    let mut job = MeteredDelimiterPageJob::new(first.token, activation.job, &b"abc"[..]);
    let ParseWorkAvailability::Work(permit) = scheduler.issue_measured_parse_slice().unwrap()
    else {
        panic!("activated parse must issue a permit");
    };
    let receipt = job.poll_measured(permit, &mut lifetime).unwrap();
    assert_eq!(receipt.progressed_units, 1);

    let second = scheduler
        .submit_measured_source_operation(next_operation(&scheduler, 2))
        .unwrap();
    assert!(matches!(
        scheduler.adopt_measured_parse_slice(permit, receipt),
        Err(Error::StaleParseGeneration { .. })
    ));
    assert_eq!(scheduler.grammar_revision(), GrammarRevision(0));

    let ParseWorkAvailability::Status(promotion) = scheduler.issue_measured_parse_slice().unwrap()
    else {
        panic!("latest-wins promotion must precede new work");
    };
    assert_eq!(promotion.cancelled, Some(first.token));
    assert_eq!(promotion.promoted, Some(second.token));
    reclaim_all(&mut scheduler, &mut lifetime);
    assert_eq!(lifetime.active_job(), None);
    scheduler.validate_invariants().unwrap();
}

#[test]
fn current_shared_lexer_and_grammar_are_explicitly_not_receipt_admissible() {
    assert_eq!(
        GRAMMAR_EXECUTION_GAPS,
        &[
            GrammarExecutionGap::LexerHasNoCompletePerPollReceipt,
            GrammarExecutionGap::GrammarReceiptUnavailableWhilePending,
            GrammarExecutionGap::GrammarAtomicResolverCannotBePreflighted,
            GrammarExecutionGap::GrammarReportsUnmeteredUpstreamAllocations,
            GrammarExecutionGap::GrammarPagesCannotEnterPhysicalArena,
        ]
    );
    assert!(EXECUTION_PROTOCOL_GAPS.is_empty());
}

#[test]
fn measured_activation_cannot_bypass_its_permit_and_growth_failure_is_preflighted() {
    let (mut lifetime, mut scheduler) = initialize(limits(u64::MAX));
    let forecast = scheduler
        .submit_source_operation(
            next_operation(&scheduler, 1),
            ParseSpec {
                work_units: 1,
                unit_audit: Audit {
                    transitions: 1,
                    ..Audit::ZERO
                },
            },
        )
        .unwrap();
    let activation = lifetime
        .activate_scheduler_job(&mut scheduler, forecast.token, b"")
        .unwrap();
    assert_eq!(
        scheduler.run_parse_slice().unwrap().status,
        ParseSliceStatus::ReadyToSeal
    );
    // Initial committed root + 255 pages in this forecast candidate fill the
    // first 256-slot slab exactly.
    for _ in 0..254 {
        lifetime.append_job_page(activation.job, &[]).unwrap();
    }
    lifetime
        .seal_scheduler_job(&mut scheduler, forecast.token, 1)
        .unwrap();
    scheduler.commit_sealed(forecast.token).unwrap();
    assert_eq!(lifetime.arena_metrics().live_nodes, 256);
    assert_eq!(lifetime.arena_metrics().slabs, 1);

    let measured = scheduler
        .submit_measured_source_operation(next_operation(&scheduler, 2))
        .unwrap();
    let before = lifetime.arena_metrics();
    assert!(matches!(
        lifetime.activate_scheduler_job(&mut scheduler, measured.token, b""),
        Err(LifetimeError::Scheduler(Error::MeasuredParseRequiresPermit))
    ));
    assert_eq!(
        lifetime.arena_metrics(),
        before,
        "legacy activation must reject measured work before allocating"
    );

    // The next anchor needs a second slab. Its exact constant-size activation
    // is 259 transitions: node allocation + slab allocation + 256 initialized
    // slots + atomic scheduler binding. The configured three-transition
    // capability rejects it before mutation.
    assert!(matches!(
        run_measured_activation(&mut scheduler, &mut lifetime, b""),
        Err(LifetimeError::Scheduler(
            Error::JobActivationReceiptExceedsLimits {
                audit: Audit {
                    transitions: 259,
                    ..
                },
                ..
            }
        ))
    ));
    assert_eq!(lifetime.arena_metrics(), before);

    // Failure consumed the one-shot permit, so the protocol is retryable rather
    // than wedged (the unchanged tiny configuration will predictably fail again).
    assert!(matches!(
        scheduler.issue_measured_job_activation().unwrap(),
        flark_integrated_parser_slice::scheduler::JobActivationAvailability::Permit(_)
    ));
    scheduler.validate_invariants().unwrap();
}

#[test]
fn job_error_after_physical_mutation_reports_audit_and_cannot_wedge_permit() {
    let (mut lifetime, mut scheduler) = initialize(limits(u64::MAX));
    let submission = scheduler
        .submit_measured_source_operation(next_operation(&scheduler, 1))
        .unwrap();
    let activation = activate_measured(&mut scheduler, &mut lifetime, b"candidate");
    let mut job = FailAfterPageJob {
        token: submission.token,
        arena_job: activation.job,
    };

    let error = run_measured_slice(&mut scheduler, &mut lifetime, &mut job).unwrap_err();
    assert!(matches!(
        error,
        ExecutionError::Job(JobPollFailure {
            error: TestJobError,
            receipt: MeasuredParseReceipt {
                audit: Audit {
                    copied_bytes: 8,
                    ..
                },
                ..
            }
        })
    ));
    assert_eq!(lifetime.active_job_pages(), 2);
    assert!(matches!(
        scheduler.issue_measured_parse_slice(),
        Err(Error::MeasuredParsePoisoned)
    ));
    scheduler.retry_poisoned_measured_parse().unwrap();
    reclaim_all(&mut scheduler, &mut lifetime);
    assert_eq!(lifetime.active_job(), None);
    assert_eq!(lifetime.arena_metrics().live_nodes, 1);
}

#[test]
fn local_job_identity_mismatch_consumes_permit_and_enters_bounded_retry_path() {
    let (mut lifetime, mut scheduler) = initialize(limits(u64::MAX));
    let submission = scheduler
        .submit_measured_source_operation(next_operation(&scheduler, 1))
        .unwrap();
    let activation = activate_measured(&mut scheduler, &mut lifetime, b"candidate");
    let mut wrong_job = MeteredDelimiterPageJob::new(
        submission.token,
        ArenaJobId(activation.job.0 + 1),
        Box::<[u8]>::default(),
    );

    assert!(matches!(
        run_measured_slice(&mut scheduler, &mut lifetime, &mut wrong_job),
        Err(ExecutionError::JobArenaMismatch { .. })
    ));
    assert!(matches!(
        scheduler.issue_measured_parse_slice(),
        Err(Error::MeasuredParsePoisoned)
    ));
    scheduler.retry_poisoned_measured_parse().unwrap();
    reclaim_all(&mut scheduler, &mut lifetime);
    assert_eq!(lifetime.active_job(), None);
}

#[test]
fn completion_mismatch_consumes_permit_and_preserves_partial_receipt() {
    let (mut lifetime, mut scheduler) = initialize(limits(u64::MAX));
    let submission = scheduler
        .submit_measured_source_operation(next_operation(&scheduler, 1))
        .unwrap();
    let activation = activate_measured(&mut scheduler, &mut lifetime, b"candidate");
    let mut job = CompletionMismatchJob {
        token: submission.token,
        arena_job: activation.job,
    };

    assert!(matches!(
        run_measured_slice(&mut scheduler, &mut lifetime, &mut job),
        Err(ExecutionError::JobCompletionMismatch {
            receipt: MeasuredParseReceipt {
                progressed_units: 0,
                audit: Audit::ZERO,
                complete: false
            }
        })
    ));
    assert!(matches!(
        scheduler.issue_measured_parse_slice(),
        Err(Error::MeasuredParsePoisoned)
    ));
    scheduler.retry_poisoned_measured_parse().unwrap();
    reclaim_all(&mut scheduler, &mut lifetime);
}

#[test]
#[allow(clippy::too_many_lines)]
fn measured_4097_page_job_crosses_slabs_retries_same_source_and_supersedes_boundedly() {
    const LARGE_PAGES: u64 = 4_097;
    let slice_limits = SliceLimits {
        source_bytes: 1,
        // A normal parent costs two transitions. A slab boundary additionally
        // initializes 256 slots and charges the slab allocation itself.
        transitions: 300,
        allocated_bytes: 64 * 1024,
        copied_bytes: 8,
        hashed_bytes: 1,
        index_nodes: 300,
        reclaimed_nodes: 31,
    };
    let (mut lifetime, mut scheduler) = initialize(slice_limits);
    let first = scheduler
        .submit_measured_source_operation(next_operation(&scheduler, 1))
        .unwrap();
    let first_activation = activate_measured(&mut scheduler, &mut lifetime, b"");
    let mut large = PageChainJob::new(first.token, first_activation.job, LARGE_PAGES, false);

    let mut parse_slices = 0_usize;
    while large.remaining_pages != 0 {
        let ExecutionSliceReport::Measured(report) =
            run_measured_slice(&mut scheduler, &mut lifetime, &mut large).unwrap()
        else {
            panic!("activated page-chain job must receive measured work");
        };
        assert_eq!(report.status, ParseSliceStatus::Pending);
        assert!(report.progressed_units > 0);
        assert!(report.audit.fits(slice_limits));
        parse_slices += 1;
    }
    assert!(parse_slices > 20);
    assert_eq!(large.slab_growths, 16);
    assert!(large
        .slab_growth_transition_costs
        .iter()
        .all(|&cost| cost == 259));
    assert_eq!(
        lifetime.active_job_pages(),
        usize::try_from(LARGE_PAGES).unwrap() + 1
    );
    assert_eq!(lifetime.arena_metrics().slabs, 17);

    // Mutate the physical candidate and truthfully return the real receipt,
    // but violate the 8-byte copy ceiling. Scheduler progress fail-closes and
    // the same immutable source snapshot becomes retryable.
    let mut overrun = OverrunPageJob {
        token: first.token,
        arena_job: first_activation.job,
        ready: false,
    };
    assert!(matches!(
        run_measured_slice(&mut scheduler, &mut lifetime, &mut overrun),
        Err(ExecutionError::Scheduler(
            Error::MeasuredParseReceiptExceedsLimits {
                audit: Audit {
                    copied_bytes: 16,
                    ..
                },
                ..
            }
        ))
    ));
    assert_eq!(
        lifetime.active_job_pages(),
        usize::try_from(LARGE_PAGES).unwrap() + 2
    );
    let source_revision = scheduler.source_revision();
    let source_root = scheduler.source_root();
    let retry = scheduler.retry_poisoned_measured_parse().unwrap();
    let retry_token = retry.promoted.expect("same-source retry is promoted");
    assert_eq!(retry.cancelled, Some(first.token));
    assert_eq!(retry_token.target_revision(), first.token.target_revision());
    assert_ne!(retry_token.generation(), first.token.generation());
    assert_eq!(scheduler.source_revision(), source_revision);
    assert_eq!(scheduler.source_root(), source_root);

    let first_reclaim = scheduler.run_reclaim_slice(&mut lifetime).unwrap();
    assert_eq!(first_reclaim.completed_tickets, 0);
    assert_eq!(first_reclaim.audit.reclaimed_nodes, 31);
    assert!(first_reclaim.audit.fits(slice_limits));
    assert_eq!(lifetime.active_job(), None);
    assert!(lifetime.arena_metrics().pending_releases > 0);

    // The retry activates and completes while the obsolete 4K-page chain is
    // still being iteratively destroyed.
    let retry_activation = activate_measured(&mut scheduler, &mut lifetime, b"");
    let mut retry_job = PageChainJob::new(retry_token, retry_activation.job, 1, true);
    let ExecutionSliceReport::Measured(retry_report) =
        run_measured_slice(&mut scheduler, &mut lifetime, &mut retry_job).unwrap()
    else {
        panic!("same-source retry must receive measured work");
    };
    assert_eq!(retry_report.status, ParseSliceStatus::ReadyToSeal);
    assert!(lifetime.arena_metrics().pending_releases > 0);

    // A newer exact source root then supersedes the completed-but-unsealed
    // retry. Its job queues behind the already-running large reclaim ticket.
    let latest = scheduler
        .submit_measured_source_operation(next_operation(&scheduler, 2))
        .unwrap();
    let ActivationSliceReport::Status(promotion) =
        run_measured_activation(&mut scheduler, &mut lifetime, b"").unwrap()
    else {
        panic!("supersession must be observed before latest activation");
    };
    assert_eq!(promotion.cancelled, Some(retry_token));
    assert_eq!(promotion.promoted, Some(latest.token));
    assert_eq!(scheduler.pending_reclaim_tickets(), 2);

    let mut reclaim_slices = 1_usize;
    while scheduler.pending_reclaim_tickets() != 0 {
        let report = scheduler.run_reclaim_slice(&mut lifetime).unwrap();
        assert!(report.audit.fits(slice_limits));
        assert!(report.audit.reclaimed_nodes <= 31);
        reclaim_slices += 1;
    }
    assert!(reclaim_slices > 130);
    assert_eq!(lifetime.arena_metrics().pending_releases, 0);
    assert_eq!(lifetime.arena_metrics().live_nodes, 1);

    let latest_activation = activate_measured(&mut scheduler, &mut lifetime, b"");
    let mut latest_job = PageChainJob::new(latest.token, latest_activation.job, 1, true);
    let ExecutionSliceReport::Measured(latest_report) =
        run_measured_slice(&mut scheduler, &mut lifetime, &mut latest_job).unwrap()
    else {
        panic!("latest generation must receive measured work");
    };
    assert_eq!(latest_report.status, ParseSliceStatus::ReadyToSeal);
    lifetime
        .seal_scheduler_job(&mut scheduler, latest.token, 1)
        .unwrap();
    let delta = scheduler.commit_sealed(latest.token).unwrap();
    assert_eq!(delta.target_source_revision, SourceRevision(2));
    assert_eq!(scheduler.grammar_revision(), GrammarRevision(2));
    scheduler.validate_invariants().unwrap();
}

#[derive(Debug)]
struct OverrunPageJob {
    token: ParseToken,
    arena_job: ArenaJobId,
    ready: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TestJobError;

impl fmt::Display for TestJobError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("injected failure after allocation")
    }
}

impl std::error::Error for TestJobError {}

#[derive(Debug)]
struct FailAfterPageJob {
    token: ParseToken,
    arena_job: ArenaJobId,
}

#[derive(Debug)]
struct CompletionMismatchJob {
    token: ParseToken,
    arena_job: ArenaJobId,
}

#[derive(Debug)]
struct PageChainJob {
    token: ParseToken,
    arena_job: ArenaJobId,
    remaining_pages: u64,
    complete_when_done: bool,
    ready: bool,
    slab_growths: usize,
    slab_growth_transition_costs: Vec<u64>,
}

impl PageChainJob {
    fn new(token: ParseToken, arena_job: ArenaJobId, pages: u64, complete_when_done: bool) -> Self {
        Self {
            token,
            arena_job,
            remaining_pages: pages,
            complete_when_done,
            ready: false,
            slab_growths: 0,
            slab_growth_transition_costs: Vec::new(),
        }
    }
}

impl MeasuredParseJob for PageChainJob {
    type Error = LifetimeError;

    fn token(&self) -> ParseToken {
        self.token
    }

    fn arena_job(&self) -> ArenaJobId {
        self.arena_job
    }

    fn is_ready(&self) -> bool {
        self.ready
    }

    fn poll_measured(
        &mut self,
        permit: ParseWorkPermit,
        lifetime: &mut PhysicalLifetime,
    ) -> Result<MeasuredParseReceipt, JobPollFailure<Self::Error>> {
        let mut audit = Audit::ZERO;
        let mut progressed_units = 0_u64;
        while self.remaining_pages != 0 {
            let remaining = subtract_audit(permit.limits(), audit)
                .expect("accepted page receipts remain inside the permit");
            let allocation = lifetime
                .try_append_job_page_under_limits(self.arena_job, &[], remaining, permit.limits())
                .map_err(|error| JobPollFailure {
                    error,
                    receipt: MeasuredParseReceipt {
                        progressed_units,
                        audit,
                        complete: false,
                    },
                })?;
            let Some(allocation) = allocation else {
                break;
            };
            let step = allocation.audit();
            if allocation.allocation.slabs_added != 0 {
                self.slab_growths += 1;
                self.slab_growth_transition_costs.push(step.transitions);
            }
            audit = add_audit(audit, step).expect("preflight proved audit addition");
            progressed_units += 1;
            self.remaining_pages -= 1;
        }
        if self.remaining_pages == 0 && self.complete_when_done {
            self.ready = true;
        }
        Ok(MeasuredParseReceipt {
            progressed_units,
            audit,
            complete: self.ready,
        })
    }
}

fn add_audit(left: Audit, right: Audit) -> Option<Audit> {
    Some(Audit {
        source_bytes: left.source_bytes.checked_add(right.source_bytes)?,
        transitions: left.transitions.checked_add(right.transitions)?,
        allocated_bytes: left.allocated_bytes.checked_add(right.allocated_bytes)?,
        copied_bytes: left.copied_bytes.checked_add(right.copied_bytes)?,
        hashed_bytes: left.hashed_bytes.checked_add(right.hashed_bytes)?,
        index_nodes: left.index_nodes.checked_add(right.index_nodes)?,
        reclaimed_nodes: left.reclaimed_nodes.checked_add(right.reclaimed_nodes)?,
    })
}

fn subtract_audit(limits: SliceLimits, audit: Audit) -> Option<SliceLimits> {
    Some(SliceLimits {
        source_bytes: limits.source_bytes.checked_sub(audit.source_bytes)?,
        transitions: limits.transitions.checked_sub(audit.transitions)?,
        allocated_bytes: limits.allocated_bytes.checked_sub(audit.allocated_bytes)?,
        copied_bytes: limits.copied_bytes.checked_sub(audit.copied_bytes)?,
        hashed_bytes: limits.hashed_bytes.checked_sub(audit.hashed_bytes)?,
        index_nodes: limits.index_nodes.checked_sub(audit.index_nodes)?,
        reclaimed_nodes: limits.reclaimed_nodes.checked_sub(audit.reclaimed_nodes)?,
    })
}

impl MeasuredParseJob for CompletionMismatchJob {
    type Error = TestJobError;

    fn token(&self) -> ParseToken {
        self.token
    }

    fn arena_job(&self) -> ArenaJobId {
        self.arena_job
    }

    fn is_ready(&self) -> bool {
        true
    }

    fn poll_measured(
        &mut self,
        _permit: ParseWorkPermit,
        _lifetime: &mut PhysicalLifetime,
    ) -> Result<MeasuredParseReceipt, JobPollFailure<Self::Error>> {
        Ok(MeasuredParseReceipt {
            progressed_units: 0,
            audit: Audit::ZERO,
            complete: false,
        })
    }
}

impl MeasuredParseJob for FailAfterPageJob {
    type Error = TestJobError;

    fn token(&self) -> ParseToken {
        self.token
    }

    fn arena_job(&self) -> ArenaJobId {
        self.arena_job
    }

    fn is_ready(&self) -> bool {
        false
    }

    fn poll_measured(
        &mut self,
        _permit: ParseWorkPermit,
        lifetime: &mut PhysicalLifetime,
    ) -> Result<MeasuredParseReceipt, JobPollFailure<Self::Error>> {
        let allocation = lifetime.append_job_page(self.arena_job, &[0; 8]).unwrap();
        Err(JobPollFailure {
            error: TestJobError,
            receipt: MeasuredParseReceipt {
                progressed_units: 1,
                audit: allocation.audit(),
                complete: false,
            },
        })
    }
}

impl MeasuredParseJob for OverrunPageJob {
    type Error = LifetimeError;

    fn token(&self) -> ParseToken {
        self.token
    }

    fn arena_job(&self) -> ArenaJobId {
        self.arena_job
    }

    fn is_ready(&self) -> bool {
        self.ready
    }

    fn poll_measured(
        &mut self,
        _permit: ParseWorkPermit,
        lifetime: &mut PhysicalLifetime,
    ) -> Result<MeasuredParseReceipt, JobPollFailure<Self::Error>> {
        let allocation = lifetime
            .append_job_page(self.arena_job, &[0; 16])
            .map_err(|error| JobPollFailure {
                error,
                receipt: MeasuredParseReceipt {
                    progressed_units: 0,
                    audit: Audit::ZERO,
                    complete: false,
                },
            })?;
        self.ready = true;
        Ok(MeasuredParseReceipt {
            progressed_units: 1,
            audit: allocation.audit(),
            complete: true,
        })
    }
}
