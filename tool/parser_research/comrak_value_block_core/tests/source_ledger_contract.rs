use flark_comrak_value_block_core::source_ledger::{
    AtomicProjection, AuthoritySubject, BoundaryAffinity, CoveragePart, LedgerError, LineLedger,
    LineLedgerFinish, LineLedgerReceipt, LogicalAction, LogicalActionError, PendingKind,
    PendingLineLedger, PendingResolutionFailure, ProjectionProgramRecipe, SemanticKind,
    SourceClaim, SourceLineError, SourceMetric, SourceRevision, SourceRootAuthority,
    SourceSpanError, TerminatorResolution,
};

fn golden_contract(root: &SourceRootAuthority) -> (Vec<String>, String) {
    let mut revision = root.begin_revision(SourceRevision(7));
    let _document = revision
        .open_binding(SemanticKind::DOCUMENT)
        .expect("document binding");
    let quote = revision
        .open_binding(SemanticKind::BLOCK_QUOTE)
        .expect("quote binding");
    let paragraph = revision
        .open_binding(SemanticKind::PARAGRAPH)
        .expect("paragraph binding");
    let line = revision
        .lease_line(4, 100, "> \tα\r\n")
        .expect("one physical line");
    let mut ledger = LineLedger::begin(&line);
    let mut golden = Vec::new();

    let marker = ledger
        .claim(SourceClaim::new(
            line.span(0..2).expect("quote marker"),
            &quote,
            CoveragePart::ContainerMarker,
            LogicalAction::none(),
            BoundaryAffinity::Downstream,
        ))
        .expect("marker claim");
    golden.push(marker.golden_debug().to_string());

    let tab = ledger
        .claim(SourceClaim::new(
            line.span(2..3).expect("tab byte"),
            &paragraph,
            CoveragePart::Content,
            LogicalAction::atomic(&paragraph, AtomicProjection::TabToSpaces { spaces: 2 })
                .expect("typed partial-tab projection"),
            BoundaryAffinity::Downstream,
        ))
        .expect("partial-tab claim");
    golden.push(tab.golden_debug().to_string());

    let content = ledger
        .claim(SourceClaim::new(
            line.span(3..5).expect("unicode content"),
            &paragraph,
            CoveragePart::Content,
            LogicalAction::identity(&paragraph).expect("inline target"),
            BoundaryAffinity::Downstream,
        ))
        .expect("content claim");
    golden.push(content.golden_debug().to_string());

    ledger
        .stage_pending_terminator(
            line.span(5..7).expect("CRLF"),
            &paragraph,
            BoundaryAffinity::Upstream,
        )
        .expect("typed pending terminator");
    let LineLedgerFinish::Pending(pending) = ledger.finish_line().expect("covered line") else {
        panic!("terminator must remain pending")
    };
    assert_eq!(pending.kind(), PendingKind::Terminator);
    let resolved = pending
        .resolve_terminator(TerminatorResolution::CloseNone)
        .expect("final paragraph terminator is physical-only");
    golden.push(resolved.claim.golden_debug().to_string());
    (golden, format!("{:?}", resolved.receipt))
}

#[test]
fn streaming_claims_have_a_deterministic_golden_form_and_digest() {
    let first = golden_contract(&SourceRootAuthority::new());
    let second = golden_contract(&SourceRootAuthority::new());
    assert_eq!(
        first, second,
        "opaque root/snapshot IDs never enter goldens"
    );
    assert_eq!(
        first.0,
        [
            "rev=7 line=4 rel=0..2 abs=100..102 metric=2b/2u16 owner=block-quote#2 part=container-marker logical=none affinity=downstream",
            "rev=7 line=4 rel=2..3 abs=102..103 metric=1b/1u16 owner=paragraph#3 part=content logical=atomic(tab-to-2-spaces)->paragraph#3 affinity=downstream",
            "rev=7 line=4 rel=3..5 abs=103..105 metric=2b/1u16 owner=paragraph#3 part=content logical=identity->paragraph#3 affinity=downstream",
            "rev=7 line=4 rel=5..7 abs=105..107 metric=2b/2u16 owner=paragraph#3 part=terminal logical=none affinity=upstream",
        ]
    );
    assert!(first.1.contains("schema_version: 1"));
    assert!(
        first
            .1
            .contains("metric: SourceMetric { bytes: 7, utf16: 6 }")
    );
    assert!(first.1.contains("claim_count: 4"));
    assert!(first.1.contains("claim_digest: 0x"));
}

#[test]
fn validator_and_pending_state_have_fixed_bounded_footprints() {
    assert!(std::mem::size_of::<LineLedger<'static>>() <= 256);
    assert!(std::mem::size_of::<PendingLineLedger<'static>>() <= 320);
    assert!(std::mem::size_of::<LineLedgerReceipt>() <= 64);
}

#[test]
fn overlap_gap_and_incomplete_tail_fail_closed_and_poison_the_ledger() {
    let root = SourceRootAuthority::new();
    let mut revision = root.begin_revision(SourceRevision(0));
    let paragraph = revision
        .open_binding(SemanticKind::PARAGRAPH)
        .expect("paragraph");
    let line = revision.lease_line(0, 0, "abc").expect("line");

    let mut overlap = LineLedger::begin(&line);
    overlap
        .claim(SourceClaim::new(
            line.span(0..2).expect("first range"),
            &paragraph,
            CoveragePart::Content,
            LogicalAction::identity(&paragraph).expect("target"),
            BoundaryAffinity::Downstream,
        ))
        .expect("first claim");
    assert_eq!(
        overlap.claim(SourceClaim::new(
            line.span(1..3).expect("overlap range"),
            &paragraph,
            CoveragePart::Content,
            LogicalAction::identity(&paragraph).expect("target"),
            BoundaryAffinity::Downstream,
        )),
        Err(LedgerError::Overlap {
            claimed_start: 1,
            next_unclaimed: 2,
        })
    );
    assert_eq!(overlap.finish_line().unwrap_err(), LedgerError::Poisoned);

    let mut gap = LineLedger::begin(&line);
    assert_eq!(
        gap.claim(SourceClaim::new(
            line.span(1..2).expect("late range"),
            &paragraph,
            CoveragePart::Content,
            LogicalAction::identity(&paragraph).expect("target"),
            BoundaryAffinity::Downstream,
        )),
        Err(LedgerError::GapBeforeClaim {
            claimed_start: 1,
            next_unclaimed: 0,
        })
    );
    assert_eq!(gap.finish_line().unwrap_err(), LedgerError::Poisoned);

    let mut incomplete = LineLedger::begin(&line);
    incomplete
        .claim(SourceClaim::new(
            line.span(0..1).expect("prefix"),
            &paragraph,
            CoveragePart::Content,
            LogicalAction::identity(&paragraph).expect("target"),
            BoundaryAffinity::Downstream,
        ))
        .expect("prefix claim");
    assert_eq!(
        incomplete.finish_line().unwrap_err(),
        LedgerError::IncompleteCoverage {
            next_unclaimed: 1,
            line_bytes: 3,
        }
    );
}

#[test]
fn root_revision_snapshot_line_owner_and_target_scopes_are_distinct() {
    let root_a = SourceRootAuthority::new();
    let root_b = SourceRootAuthority::new();
    let mut a = root_a.begin_revision(SourceRevision(9));
    let mut b = root_b.begin_revision(SourceRevision(9));
    let paragraph_a = a
        .open_binding(SemanticKind::PARAGRAPH)
        .expect("paragraph A");
    let paragraph_b = b
        .open_binding(SemanticKind::PARAGRAPH)
        .expect("paragraph B");
    let line_a = a.lease_line(1, 20, "x").expect("line A");
    let line_b = b.lease_line(1, 20, "x").expect("line B");

    let mut wrong_span_root = LineLedger::begin(&line_a);
    assert_eq!(
        wrong_span_root.claim(SourceClaim::new(
            line_b.span(0..1).expect("span B"),
            &paragraph_a,
            CoveragePart::Content,
            LogicalAction::identity(&paragraph_a).expect("target A"),
            BoundaryAffinity::Downstream,
        )),
        Err(LedgerError::WrongSourceRoot {
            subject: AuthoritySubject::SourceSpan,
        })
    );

    let mut wrong_owner_root = LineLedger::begin(&line_a);
    assert_eq!(
        wrong_owner_root.claim(SourceClaim::new(
            line_a.span(0..1).expect("span A"),
            &paragraph_b,
            CoveragePart::Content,
            LogicalAction::none(),
            BoundaryAffinity::Downstream,
        )),
        Err(LedgerError::WrongSourceRoot {
            subject: AuthoritySubject::PhysicalOwner,
        })
    );

    let mut wrong_target_root = LineLedger::begin(&line_a);
    assert_eq!(
        wrong_target_root.claim(SourceClaim::new(
            line_a.span(0..1).expect("span A"),
            &paragraph_a,
            CoveragePart::Content,
            LogicalAction::identity(&paragraph_b).expect("target B"),
            BoundaryAffinity::Downstream,
        )),
        Err(LedgerError::WrongSourceRoot {
            subject: AuthoritySubject::LogicalTarget,
        })
    );

    let mut other_revision = root_a.begin_revision(SourceRevision(10));
    let paragraph_revision_10 = other_revision
        .open_binding(SemanticKind::PARAGRAPH)
        .expect("revision 10 paragraph");
    let mut wrong_revision = LineLedger::begin(&line_a);
    assert_eq!(
        wrong_revision.claim(SourceClaim::new(
            line_a.span(0..1).expect("span A"),
            &paragraph_revision_10,
            CoveragePart::Content,
            LogicalAction::none(),
            BoundaryAffinity::Downstream,
        )),
        Err(LedgerError::WrongRevision {
            subject: AuthoritySubject::PhysicalOwner,
            expected: SourceRevision(9),
            actual: SourceRevision(10),
        })
    );

    let mut same_revision_new_snapshot = root_a.begin_revision(SourceRevision(9));
    let colliding_local_id = same_revision_new_snapshot
        .open_binding(SemanticKind::PARAGRAPH)
        .expect("same local ID in another snapshot");
    let mut wrong_snapshot = LineLedger::begin(&line_a);
    assert_eq!(
        wrong_snapshot.claim(SourceClaim::new(
            line_a.span(0..1).expect("span A"),
            &colliding_local_id,
            CoveragePart::Content,
            LogicalAction::none(),
            BoundaryAffinity::Downstream,
        )),
        Err(LedgerError::WrongSnapshot {
            subject: AuthoritySubject::PhysicalOwner,
        })
    );

    let other_line = a.lease_line(2, 21, "x").expect("another line A");
    let mut wrong_line = LineLedger::begin(&line_a);
    assert_eq!(
        wrong_line.claim(SourceClaim::new(
            other_line.span(0..1).expect("other line span"),
            &paragraph_a,
            CoveragePart::Content,
            LogicalAction::none(),
            BoundaryAffinity::Downstream,
        )),
        Err(LedgerError::WrongSourceLine)
    );
}

#[test]
fn unicode_metrics_are_source_derived_and_utf8_interior_cuts_are_rejected() {
    let root = SourceRootAuthority::new();
    let mut revision = root.begin_revision(SourceRevision(3));
    let paragraph = revision
        .open_binding(SemanticKind::PARAGRAPH)
        .expect("paragraph");
    let text = "a😀β\r\n";
    assert_eq!(
        revision
            .lease_line_with_metric(0, 0, text, SourceMetric { bytes: 9, utf16: 5 },)
            .unwrap_err(),
        SourceLineError::MetricMismatch {
            source: SourceMetric { bytes: 9, utf16: 5 },
            derived: SourceMetric { bytes: 9, utf16: 6 },
        }
    );
    let line = revision.lease_line(0, 0, text).expect("unicode line");
    assert_eq!(
        line.span(2..5).unwrap_err(),
        SourceSpanError::NotUtf8Boundary { range: 2..5 }
    );

    let mut ledger = LineLedger::begin(&line);
    let mut metrics = Vec::new();
    for range in [0..1, 1..5, 5..7] {
        metrics.push(
            ledger
                .claim(SourceClaim::new(
                    line.span(range).expect("character boundary"),
                    &paragraph,
                    CoveragePart::Content,
                    LogicalAction::identity(&paragraph).expect("target"),
                    BoundaryAffinity::Downstream,
                ))
                .expect("unicode claim")
                .metric(),
        );
    }
    ledger
        .stage_pending_terminator(
            line.span(7..9).expect("CRLF"),
            &paragraph,
            BoundaryAffinity::Upstream,
        )
        .expect("pending terminator");
    let LineLedgerFinish::Pending(pending) = ledger.finish_line().expect("covered") else {
        panic!("expected pending CRLF")
    };
    let resolved = pending
        .resolve_terminator(TerminatorResolution::CloseNone)
        .expect("close terminator");
    metrics.push(resolved.claim.metric());
    assert_eq!(
        metrics,
        [
            SourceMetric { bytes: 1, utf16: 1 },
            SourceMetric { bytes: 4, utf16: 2 },
            SourceMetric { bytes: 2, utf16: 1 },
            SourceMetric { bytes: 2, utf16: 2 },
        ]
    );
    assert_eq!(
        resolved.receipt.metric(),
        SourceMetric { bytes: 9, utf16: 6 }
    );
}

#[test]
fn partial_tab_and_nul_transforms_validate_the_exact_physical_byte() {
    let root = SourceRootAuthority::new();
    let mut revision = root.begin_revision(SourceRevision(1));
    let paragraph = revision
        .open_binding(SemanticKind::PARAGRAPH)
        .expect("paragraph");
    assert_eq!(
        LogicalAction::atomic(&paragraph, AtomicProjection::TabToSpaces { spaces: 0 }),
        Err(LogicalActionError::InvalidTabExpansion(0))
    );

    let line = revision.lease_line(0, 0, "\t\0x").expect("tab/NUL line");
    let mut ledger = LineLedger::begin(&line);
    ledger
        .claim(SourceClaim::new(
            line.span(0..1).expect("tab"),
            &paragraph,
            CoveragePart::Content,
            LogicalAction::atomic(&paragraph, AtomicProjection::TabToSpaces { spaces: 3 })
                .expect("partial tab"),
            BoundaryAffinity::Downstream,
        ))
        .expect("tab source byte");
    ledger
        .claim(SourceClaim::new(
            line.span(1..2).expect("NUL"),
            &paragraph,
            CoveragePart::Content,
            LogicalAction::atomic(&paragraph, AtomicProjection::NulToReplacement)
                .expect("NUL replacement"),
            BoundaryAffinity::Downstream,
        ))
        .expect("NUL source byte");
    ledger
        .claim(SourceClaim::new(
            line.span(2..3).expect("x"),
            &paragraph,
            CoveragePart::Content,
            LogicalAction::identity(&paragraph).expect("target"),
            BoundaryAffinity::Downstream,
        ))
        .expect("identity x");
    let LineLedgerFinish::Complete(receipt) = ledger.finish_line().expect("complete line") else {
        panic!("no pending tail")
    };
    assert_eq!(receipt.claim_count(), 3);

    let space = revision.lease_line(1, 3, " ").expect("space line");
    let mut invalid_tab = LineLedger::begin(&space);
    assert_eq!(
        invalid_tab.claim(SourceClaim::new(
            space.span(0..1).expect("space"),
            &paragraph,
            CoveragePart::Content,
            LogicalAction::atomic(&paragraph, AtomicProjection::TabToSpaces { spaces: 1 },)
                .expect("valid recipe, wrong physical byte"),
            BoundaryAffinity::Downstream,
        )),
        Err(LedgerError::InvalidAtomicPhysicalInput(
            AtomicProjection::TabToSpaces { spaces: 1 }
        ))
    );

    let x = revision.lease_line(2, 4, "x").expect("x line");
    let mut invalid_nul = LineLedger::begin(&x);
    assert_eq!(
        invalid_nul.claim(SourceClaim::new(
            x.span(0..1).expect("x"),
            &paragraph,
            CoveragePart::Content,
            LogicalAction::atomic(&paragraph, AtomicProjection::NulToReplacement)
                .expect("valid NUL recipe"),
            BoundaryAffinity::Downstream,
        )),
        Err(LedgerError::InvalidAtomicPhysicalInput(
            AtomicProjection::NulToReplacement
        ))
    );
}

#[test]
fn pending_gap_resolution_is_typed_snapshot_bound_and_recoverable() {
    let root = SourceRootAuthority::new();
    let other_root = SourceRootAuthority::new();
    let mut revision = root.begin_revision(SourceRevision(2));
    let mut other_revision = other_root.begin_revision(SourceRevision(2));
    let document = revision
        .open_binding(SemanticKind::DOCUMENT)
        .expect("document");
    let wrong_document = other_revision
        .open_binding(SemanticKind::DOCUMENT)
        .expect("other document");
    let line = revision.lease_line(8, 55, " \t\r\n").expect("blank line");
    let mut ledger = LineLedger::begin(&line);
    ledger
        .stage_pending_gap(
            line.span(0..4).expect("blank tail"),
            BoundaryAffinity::Upstream,
        )
        .expect("pending gap");
    let LineLedgerFinish::Pending(pending) = ledger.finish_line().expect("covered blank") else {
        panic!("blank owner must remain pending")
    };
    let error = pending
        .resolve_gap(&wrong_document)
        .expect_err("wrong root cannot own pending gap");
    assert_eq!(
        error.error(),
        PendingResolutionFailure::Ledger(LedgerError::WrongSourceRoot {
            subject: AuthoritySubject::PendingOwner,
        })
    );
    let pending = error.into_pending();
    let wrong_kind = pending
        .resolve_terminator(TerminatorResolution::CloseNone)
        .expect_err("gap cannot be resolved as a terminator");
    assert_eq!(
        wrong_kind.error(),
        PendingResolutionFailure::WrongPendingKind {
            expected: PendingKind::Terminator,
            actual: PendingKind::Gap,
        }
    );
    let resolved = wrong_kind
        .into_pending()
        .resolve_gap(&document)
        .expect("surviving owner resolves gap");
    assert_eq!(resolved.claim.part(), CoveragePart::Gap);
    assert_eq!(resolved.claim.logical(), LogicalAction::none());
    assert_eq!(resolved.receipt.claim_count(), 1);

    let nonblank = revision
        .lease_line(9, 59, "secret\n")
        .expect("nonblank line");
    let mut catch_all = LineLedger::begin(&nonblank);
    assert_eq!(
        catch_all.stage_pending_gap(
            nonblank.span(0..7).expect("whole line"),
            BoundaryAffinity::Upstream,
        ),
        Err(LedgerError::PendingGapMustBeBlank)
    );
    assert_eq!(catch_all.finish_line().unwrap_err(), LedgerError::Poisoned);
}

#[test]
fn exact_lf_cr_and_crlf_terminators_resolve_by_explicit_policy() {
    for (ending, expected_logical) in [
        ("\n", "identity->paragraph#1"),
        ("\r", "atomic(cr-to-lf)->paragraph#1"),
        ("\r\n", "atomic(crlf-to-lf)->paragraph#1"),
    ] {
        let root = SourceRootAuthority::new();
        let mut revision = root.begin_revision(SourceRevision(0));
        let paragraph = revision
            .open_binding(SemanticKind::PARAGRAPH)
            .expect("paragraph");
        let text = format!("x{ending}");
        let line = revision.lease_line(0, 0, &text).expect("one line");
        let mut ledger = LineLedger::begin(&line);
        ledger
            .claim(SourceClaim::new(
                line.span(0..1).expect("x"),
                &paragraph,
                CoveragePart::Content,
                LogicalAction::identity(&paragraph).expect("target"),
                BoundaryAffinity::Downstream,
            ))
            .expect("content");
        ledger
            .stage_pending_terminator(
                line.span(1..text.len()).expect("exact ending"),
                &paragraph,
                BoundaryAffinity::Upstream,
            )
            .expect("pending terminator");
        let LineLedgerFinish::Pending(pending) = ledger.finish_line().expect("covered") else {
            panic!("terminator pending")
        };
        let continued = pending
            .resolve_terminator(TerminatorResolution::ContinueCanonicalNewline)
            .expect("continued terminal");
        assert_eq!(continued.claim.part(), CoveragePart::Content);
        assert!(
            continued
                .claim
                .golden_debug()
                .to_string()
                .contains(expected_logical)
        );
    }

    let root = SourceRootAuthority::new();
    let mut revision = root.begin_revision(SourceRevision(0));
    let paragraph = revision
        .open_binding(SemanticKind::PARAGRAPH)
        .expect("paragraph");
    let line = revision.lease_line(0, 0, "x \r\n").expect("line");
    let mut imprecise = LineLedger::begin(&line);
    imprecise
        .claim(SourceClaim::new(
            line.span(0..1).expect("x"),
            &paragraph,
            CoveragePart::Content,
            LogicalAction::identity(&paragraph).expect("target"),
            BoundaryAffinity::Downstream,
        ))
        .expect("content");
    assert_eq!(
        imprecise.stage_pending_terminator(
            line.span(1..4).expect("space plus CRLF"),
            &paragraph,
            BoundaryAffinity::Upstream,
        ),
        Err(LedgerError::PendingTerminatorMustBeExactLineEnding)
    );

    let no_ending = revision.lease_line(1, 4, "x").expect("final line");
    let mut impossible = LineLedger::begin(&no_ending);
    assert_eq!(
        impossible.stage_pending_terminator(
            no_ending.span(0..1).expect("x"),
            &paragraph,
            BoundaryAffinity::Upstream,
        ),
        Err(LedgerError::NoPhysicalLineEnding)
    );
}

#[test]
fn frozen_indent_policy_forms_a_total_ledger() {
    let root = SourceRootAuthority::new();
    let mut revision = root.begin_revision(SourceRevision(0));
    let document = revision
        .open_binding(SemanticKind::DOCUMENT)
        .expect("document");
    let paragraph = revision
        .open_binding(SemanticKind::PARAGRAPH)
        .expect("paragraph");
    let line = revision
        .lease_line(1, 6, "   beta\n")
        .expect("continuation line");
    let mut indent = LineLedger::begin(&line);
    let gap = indent
        .claim(SourceClaim::new(
            line.span(0..3).expect("stripped indent"),
            &document,
            CoveragePart::Gap,
            LogicalAction::none(),
            BoundaryAffinity::Upstream,
        ))
        .expect("document gap");
    let content = indent
        .claim(SourceClaim::new(
            line.span(3..7).expect("beta"),
            &paragraph,
            CoveragePart::Content,
            LogicalAction::identity(&paragraph).expect("target"),
            BoundaryAffinity::Downstream,
        ))
        .expect("paragraph content");
    indent
        .stage_pending_terminator(
            line.span(7..8).expect("LF"),
            &paragraph,
            BoundaryAffinity::Upstream,
        )
        .expect("final terminator");
    let LineLedgerFinish::Pending(pending) = indent.finish_line().expect("total line") else {
        panic!("paragraph terminator pending")
    };
    let final_line = pending
        .resolve_terminator(TerminatorResolution::CloseNone)
        .expect("paragraph closes");
    assert_eq!(gap.part(), CoveragePart::Gap);
    assert_eq!(gap.logical(), LogicalAction::None);
    assert_eq!(content.part(), CoveragePart::Content);
    assert_eq!(
        final_line.receipt.metric(),
        SourceMetric { bytes: 8, utf16: 8 }
    );
}

#[test]
fn frozen_nonclosing_atx_tail_policy_forms_a_total_ledger() {
    let root = SourceRootAuthority::new();
    let mut revision = root.begin_revision(SourceRevision(0));
    let heading = revision
        .open_binding(SemanticKind::HEADING)
        .expect("heading");
    let atx_line = revision
        .lease_line(0, 0, "# alpha#   \r\n")
        .expect("ATX line");
    let mut atx = LineLedger::begin(&atx_line);
    let mut atx_golden = Vec::new();
    for (range, part, logical, affinity) in [
        (
            0..2,
            CoveragePart::BlockMarker,
            LogicalAction::none(),
            BoundaryAffinity::Downstream,
        ),
        (
            2..8,
            CoveragePart::Content,
            LogicalAction::identity(&heading).expect("heading target"),
            BoundaryAffinity::Downstream,
        ),
        (
            8..11,
            CoveragePart::Content,
            LogicalAction::program(
                &heading,
                ProjectionProgramRecipe::Hidden {
                    affinity: BoundaryAffinity::Upstream,
                },
            )
            .expect("hidden tail"),
            BoundaryAffinity::Upstream,
        ),
    ] {
        atx_golden.push(
            atx.claim(SourceClaim::new(
                atx_line.span(range).expect("ATX source cut"),
                &heading,
                part,
                logical,
                affinity,
            ))
            .expect("ATX claim")
            .golden_debug()
            .to_string(),
        );
    }
    atx.stage_pending_terminator(
        atx_line.span(11..13).expect("CRLF"),
        &heading,
        BoundaryAffinity::Upstream,
    )
    .expect("ATX terminator");
    let LineLedgerFinish::Pending(pending) = atx.finish_line().expect("total ATX line") else {
        panic!("ATX terminator pending")
    };
    let resolved = pending
        .resolve_terminator(TerminatorResolution::CloseNone)
        .expect("ATX closes");
    atx_golden.push(resolved.claim.golden_debug().to_string());
    assert!(atx_golden[2].contains("part=content logical=program(hidden:upstream)"));
    assert!(atx_golden[3].contains("part=terminal logical=none"));
    assert_eq!(
        resolved.receipt.metric(),
        SourceMetric {
            bytes: 13,
            utf16: 13
        }
    );
}
