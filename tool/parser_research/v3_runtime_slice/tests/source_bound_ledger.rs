use flark_v3_runtime_slice::{
    CandidateAtomicProjection, CandidateLineEnding, CandidateLogicalAction, CandidateOpenBinding,
    CandidateRecognitionPoll, CandidateRecognitionRangeKind, CandidateSourceAtom,
    CandidateSourceAtomKind, CandidateSourcePoll, CandidateTerminatorResolution, CoveragePart,
    GreenAffinity, GreenKind, LiveCandidateEpoch, LiveDocumentError, LiveDocumentStore,
    PendingSourceKind, SourceBoundLedgerError, ValidatedLogicalKind,
};

fn activate(source: &str) -> (LiveDocumentStore, LiveCandidateEpoch) {
    let mut document = LiveDocumentStore::new(source, 8).unwrap();
    let token = document.active_parse_plan().unwrap().token;
    let epoch = document.begin_candidate(token).unwrap();
    document.activate_candidate_source_ledger(epoch).unwrap();
    (document, epoch)
}

fn open_document_and_paragraph(
    document: &mut LiveDocumentStore,
    epoch: LiveCandidateEpoch,
) -> (CandidateOpenBinding, CandidateOpenBinding) {
    let root = document
        .candidate_open_binding(epoch, GreenKind::DOCUMENT)
        .unwrap();
    let paragraph = document
        .candidate_open_binding(epoch, GreenKind::PARAGRAPH)
        .unwrap();
    (root, paragraph)
}

fn next_atom(
    document: &mut LiveDocumentStore,
    epoch: LiveCandidateEpoch,
    fuel: usize,
) -> Option<(CandidateSourceAtom, usize, usize)> {
    loop {
        match document.poll_candidate_source(epoch, fuel).unwrap() {
            CandidateSourcePoll::NeedFuel(receipt) => {
                assert_eq!(receipt.work_units, fuel);
                assert!(receipt.source_bytes_read <= receipt.work_units);
            }
            CandidateSourcePoll::Atom { atom, receipt } => {
                return Some((atom, receipt.work_units, receipt.source_bytes_read));
            }
            CandidateSourcePoll::Eof(_) => return None,
        }
    }
}

#[test]
fn activation_rejects_a_raw_cursor_that_already_consumed_source() {
    let mut document = LiveDocumentStore::new("abc", 8).unwrap();
    let token = document.active_parse_plan().unwrap().token;
    let epoch = document.begin_candidate(token).unwrap();
    assert_eq!(
        document.poll_candidate_byte(epoch).unwrap().unwrap().byte,
        b'a'
    );
    assert_eq!(document.candidate_cursor_offset(epoch).unwrap(), 1);

    assert_eq!(
        document.activate_candidate_source_ledger(epoch),
        Err(LiveDocumentError::CandidateSourceLedgerRequiresFreshCursor)
    );
    assert_eq!(document.candidate_cursor_offset(epoch).unwrap(), 1);
    assert_eq!(
        document.poll_candidate_byte(epoch).unwrap().unwrap().byte,
        b'b'
    );
}

#[test]
// One end-to-end ledger trace is intentionally kept contiguous as an
// executable protocol receipt.
#[allow(clippy::too_many_lines)]
fn unicode_and_exact_typed_atoms_form_one_ordered_total_ledger() {
    let source = "a😀\t\0\r\nβ\rz\n";
    let (mut document, epoch) = activate(source);
    let (root, paragraph) = open_document_and_paragraph(&mut document, epoch);
    let identity = CandidateLogicalAction::identity(&paragraph).unwrap();

    let (a, _, _) = next_atom(&mut document, epoch, 1).unwrap();
    assert_eq!(a.kind(), CandidateSourceAtomKind::Scalar('a'));
    let (emoji, _, _) = next_atom(&mut document, epoch, 1).unwrap();
    assert_eq!(emoji.kind(), CandidateSourceAtomKind::Scalar('😀'));
    let ordinary = document
        .candidate_claim_to(
            epoch,
            emoji.boundary(),
            &paragraph,
            CoveragePart::CONTENT,
            &identity,
            GreenAffinity::Downstream,
        )
        .unwrap();
    assert_eq!(ordinary.absolute_range(), (0, 5));
    assert_eq!(ordinary.metric().bytes(), 5);
    assert_eq!(ordinary.metric().utf16(), 3);
    assert_eq!(ordinary.logical().kind(), ValidatedLogicalKind::Identity);

    let (tab, _, _) = next_atom(&mut document, epoch, 1).unwrap();
    assert_eq!(tab.kind(), CandidateSourceAtomKind::Tab);
    let tab_action = CandidateLogicalAction::tab_to_spaces(&paragraph, &tab, 3).unwrap();
    let tab_claim = document
        .candidate_claim_to(
            epoch,
            tab.boundary(),
            &paragraph,
            CoveragePart::CONTENT,
            &tab_action,
            GreenAffinity::Downstream,
        )
        .unwrap();
    assert_eq!(
        tab_claim.logical().projection(),
        Some(CandidateAtomicProjection::TabToSpaces { spaces: 3 })
    );

    let (nul, _, _) = next_atom(&mut document, epoch, 1).unwrap();
    assert_eq!(nul.kind(), CandidateSourceAtomKind::Nul);
    let nul_action = CandidateLogicalAction::nul_to_replacement(&paragraph, &nul).unwrap();
    let nul_claim = document
        .candidate_claim_to(
            epoch,
            nul.boundary(),
            &paragraph,
            CoveragePart::CONTENT,
            &nul_action,
            GreenAffinity::Downstream,
        )
        .unwrap();
    assert_eq!(
        nul_claim.logical().projection(),
        Some(CandidateAtomicProjection::NulToReplacement)
    );

    let (crlf, _, _) = next_atom(&mut document, epoch, 1).unwrap();
    assert_eq!(
        crlf.kind(),
        CandidateSourceAtomKind::LineEnding(CandidateLineEnding::CrLf)
    );
    document
        .candidate_stage_terminator(epoch, &crlf, &paragraph, GreenAffinity::Downstream)
        .unwrap();
    assert_eq!(
        document.candidate_finish_line(epoch).unwrap().pending(),
        Some(PendingSourceKind::Terminator)
    );

    // Recognition may look into the next line, but later source output cannot
    // pass the unresolved predecessor.
    let (beta, _, _) = next_atom(&mut document, epoch, 1).unwrap();
    assert_eq!(beta.kind(), CandidateSourceAtomKind::Scalar('β'));
    assert_eq!(
        document.candidate_claim_to(
            epoch,
            beta.boundary(),
            &paragraph,
            CoveragePart::CONTENT,
            &identity,
            GreenAffinity::Downstream,
        ),
        Err(LiveDocumentError::SourceLedger(
            SourceBoundLedgerError::PreviousPendingUnresolved
        ))
    );
    let crlf_claim = document
        .candidate_resolve_terminator(
            epoch,
            CandidateTerminatorResolution::ContinueCanonicalNewline,
        )
        .unwrap();
    assert_eq!(
        crlf_claim.logical().projection(),
        Some(CandidateAtomicProjection::CrLfToLf)
    );
    document
        .candidate_claim_to(
            epoch,
            beta.boundary(),
            &paragraph,
            CoveragePart::CONTENT,
            &identity,
            GreenAffinity::Downstream,
        )
        .unwrap();

    let (lone_cr, _, read_for_lookahead) = next_atom(&mut document, epoch, 1).unwrap();
    assert_eq!(
        lone_cr.kind(),
        CandidateSourceAtomKind::LineEnding(CandidateLineEnding::LoneCr)
    );
    assert_eq!(
        read_for_lookahead, 1,
        "the following z was read once as bounded lookahead"
    );
    document
        .candidate_stage_terminator(epoch, &lone_cr, &paragraph, GreenAffinity::Downstream)
        .unwrap();
    document.candidate_finish_line(epoch).unwrap();
    let lone_cr_claim = document
        .candidate_resolve_terminator(
            epoch,
            CandidateTerminatorResolution::ContinueCanonicalNewline,
        )
        .unwrap();
    assert_eq!(
        lone_cr_claim.logical().projection(),
        Some(CandidateAtomicProjection::LoneCrToLf)
    );

    let (z, replay_work, replay_reads) = next_atom(&mut document, epoch, 1).unwrap();
    assert_eq!(z.kind(), CandidateSourceAtomKind::Scalar('z'));
    assert_eq!(replay_work, 1);
    assert_eq!(
        replay_reads, 0,
        "lookahead is replayed without rereading Crop"
    );
    document
        .candidate_claim_to(
            epoch,
            z.boundary(),
            &paragraph,
            CoveragePart::CONTENT,
            &identity,
            GreenAffinity::Downstream,
        )
        .unwrap();
    let (lf, _, _) = next_atom(&mut document, epoch, 1).unwrap();
    document
        .candidate_stage_terminator(epoch, &lf, &paragraph, GreenAffinity::Downstream)
        .unwrap();
    document.candidate_finish_line(epoch).unwrap();
    document
        .candidate_resolve_terminator(epoch, CandidateTerminatorResolution::CloseNone)
        .unwrap();

    assert!(next_atom(&mut document, epoch, 1).is_none());
    document.candidate_close_binding(epoch, &paragraph).unwrap();
    document.candidate_close_binding(epoch, &root).unwrap();
    let seal = document.seal_candidate_source(epoch).unwrap();
    assert_eq!(seal.source().bytes, source.len());
    assert_eq!(seal.metric().bytes(), u64::try_from(source.len()).unwrap());
    assert_eq!(
        seal.metric().utf16(),
        u64::try_from(source.encode_utf16().count()).unwrap()
    );
    assert_eq!(seal.authoritative_root_utf16(), seal.metric().utf16());
    assert_eq!(seal.line_count(), 3);
    assert_eq!(seal.claim_count(), 8);
    assert_eq!(seal.maximum_decoder_bytes(), 4);
    assert_eq!(seal.source_bytes_copied(), source.len());
    assert!(seal.maximum_source_chunk_bytes() <= 4 * 1024);
    assert_eq!(seal.maximum_open_path_len(), 2);
    assert!(seal.maximum_open_path_capacity_bytes() > 0);
}

#[test]
fn blank_lines_coalesce_in_o1_state_and_resolve_to_surviving_ancestry() {
    let source = " \t\r\n\t\nb";
    let (mut document, epoch) = activate(source);
    let (root, paragraph) = open_document_and_paragraph(&mut document, epoch);

    while !matches!(
        next_atom(&mut document, epoch, 1).unwrap().0.kind(),
        CandidateSourceAtomKind::LineEnding(_)
    ) {}
    document
        .candidate_stage_blank_gap(epoch, GreenAffinity::Upstream)
        .unwrap();
    document.candidate_finish_line(epoch).unwrap();

    while !matches!(
        next_atom(&mut document, epoch, 1).unwrap().0.kind(),
        CandidateSourceAtomKind::LineEnding(_)
    ) {}
    assert_eq!(
        document.candidate_stage_blank_gap(epoch, GreenAffinity::Downstream),
        Err(LiveDocumentError::SourceLedger(
            SourceBoundLedgerError::PendingGapAffinityMismatch
        ))
    );
    document
        .candidate_stage_blank_gap(epoch, GreenAffinity::Upstream)
        .unwrap();
    document.candidate_finish_line(epoch).unwrap();

    let (b, _, _) = next_atom(&mut document, epoch, 1).unwrap();
    let later_quote = document
        .candidate_open_binding(epoch, GreenKind::BLOCK_QUOTE)
        .unwrap();
    assert_eq!(
        document.candidate_resolve_blank_gap(epoch, &later_quote),
        Err(LiveDocumentError::SourceLedger(
            SourceBoundLedgerError::PendingGapOwnerOpenedAfterGap
        ))
    );
    document
        .candidate_close_binding(epoch, &later_quote)
        .unwrap();
    let gap = document.candidate_resolve_blank_gap(epoch, &root).unwrap();
    assert_eq!(gap.absolute_range(), (0, 6));
    assert_eq!(gap.metric().bytes(), 6);
    assert_eq!(gap.owner_block(), root.block_id());
    assert_eq!(gap.part(), CoveragePart::GAP);
    let identity = CandidateLogicalAction::identity(&paragraph).unwrap();
    document
        .candidate_claim_to(
            epoch,
            b.boundary(),
            &paragraph,
            CoveragePart::CONTENT,
            &identity,
            GreenAffinity::Downstream,
        )
        .unwrap();
    assert!(next_atom(&mut document, epoch, 1).is_none());
    document.candidate_finish_line(epoch).unwrap();
    document.candidate_close_binding(epoch, &paragraph).unwrap();
    document.candidate_close_binding(epoch, &root).unwrap();
    let seal = document.seal_candidate_source(epoch).unwrap();
    assert_eq!(seal.claim_count(), 2);
    assert_eq!(seal.line_count(), 3);
}

#[test]
fn giant_unicode_line_is_single_pass_with_tiny_fuel_and_bounded_scratch() {
    const SCALARS: usize = 250_000;
    let source = "😀".repeat(SCALARS);
    let (mut document, epoch) = activate(&source);
    let (root, paragraph) = open_document_and_paragraph(&mut document, epoch);
    let mut last = None;
    let mut atoms = 0usize;
    loop {
        match document.poll_candidate_source(epoch, 1).unwrap() {
            CandidateSourcePoll::NeedFuel(receipt) => {
                assert_eq!(receipt.work_units, 1);
                assert!(receipt.source_bytes_read <= 1);
            }
            CandidateSourcePoll::Atom { atom, receipt } => {
                assert_eq!(receipt.work_units, 1);
                assert!(receipt.source_bytes_read <= 1);
                last = Some(atom.boundary());
                atoms += 1;
            }
            CandidateSourcePoll::Eof(receipt) => {
                assert_eq!(receipt.work_units, 1);
                break;
            }
        }
    }
    assert_eq!(atoms, SCALARS);
    let identity = CandidateLogicalAction::identity(&paragraph).unwrap();
    let claim = document
        .candidate_claim_to(
            epoch,
            last.unwrap(),
            &paragraph,
            CoveragePart::CONTENT,
            &identity,
            GreenAffinity::Downstream,
        )
        .unwrap();
    assert_eq!(claim.metric().bytes(), u64::try_from(source.len()).unwrap());
    assert_eq!(claim.metric().utf16(), u64::try_from(SCALARS * 2).unwrap());
    document.candidate_finish_line(epoch).unwrap();
    document.candidate_close_binding(epoch, &paragraph).unwrap();
    document.candidate_close_binding(epoch, &root).unwrap();
    let seal = document.seal_candidate_source(epoch).unwrap();
    assert_eq!(seal.source_bytes_copied(), source.len());
    assert_eq!(seal.maximum_decoder_bytes(), 4);
    assert!(seal.maximum_source_chunk_bytes() <= 4 * 1024);
    assert_eq!(seal.claim_count(), 1);
}

#[test]
fn giant_line_cancellation_invalidates_every_source_and_binding_capability() {
    let source = "a".repeat(10 * 1024 * 1024);
    let mut document = LiveDocumentStore::new(&source, 8).unwrap();
    let token = document.active_parse_plan().unwrap().token;
    let first = document.begin_candidate(token).unwrap();
    document.activate_candidate_source_ledger(first).unwrap();
    let (_root, paragraph) = open_document_and_paragraph(&mut document, first);
    let identity = CandidateLogicalAction::identity(&paragraph).unwrap();
    let mut old_boundary = None;
    for _ in 0..257 {
        let (atom, work, reads) = next_atom(&mut document, first, 1).unwrap();
        assert_eq!(work, 1);
        assert_eq!(reads, 1);
        old_boundary = Some(atom.boundary());
    }
    assert_eq!(document.candidate_cursor_offset(first).unwrap(), 257);
    let abort = document.cancel_candidate(first).unwrap();
    assert_eq!(
        document.poll_candidate_source(first, 1),
        Err(LiveDocumentError::NoCandidate)
    );
    assert!(document.poll_candidate_abort(abort, 0).unwrap().complete);

    let second = document.begin_candidate(token).unwrap();
    document.activate_candidate_source_ledger(second).unwrap();
    let (_new_root, new_paragraph) = open_document_and_paragraph(&mut document, second);
    assert_eq!(
        document.candidate_claim_to(
            second,
            old_boundary.unwrap(),
            &new_paragraph,
            CoveragePart::CONTENT,
            &identity,
            GreenAffinity::Downstream,
        ),
        Err(LiveDocumentError::SourceLedger(
            SourceBoundLedgerError::WrongBindingBuild
        ))
    );
}

#[test]
fn deep_open_path_is_measured_and_stale_closed_owners_are_rejected() {
    const DEPTH: usize = 20_000;
    let (mut document, epoch) = activate("");
    let root = document
        .candidate_open_binding(epoch, GreenKind::DOCUMENT)
        .unwrap();
    let mut quotes = Vec::with_capacity(DEPTH);
    for _ in 0..DEPTH {
        quotes.push(
            document
                .candidate_open_binding(epoch, GreenKind::BLOCK_QUOTE)
                .unwrap(),
        );
    }
    while let Some(binding) = quotes.pop() {
        document.candidate_close_binding(epoch, &binding).unwrap();
    }
    document.candidate_close_binding(epoch, &root).unwrap();
    assert!(next_atom(&mut document, epoch, 1).is_none());
    let seal = document.seal_candidate_source(epoch).unwrap();
    assert_eq!(seal.maximum_open_path_len(), DEPTH + 1);
    assert!(seal.maximum_open_path_capacity_bytes() > 0);
}

#[test]
// Recognition and replay assertions belong to one contiguous stress receipt.
#[allow(clippy::too_many_lines)]
fn giant_dense_recognition_replays_exactly_without_buffering_the_line() {
    const REPEATS: usize = 400_000;
    let mut source = "a😀\t\0".repeat(REPEATS);
    source.push_str("\r\n");
    assert!(source.len() > 2 * 1024 * 1024);

    let (mut document, epoch) = activate(&source);
    let (root, paragraph) = open_document_and_paragraph(&mut document, epoch);
    let initial_checkpoint = document.candidate_recognition_checkpoint(epoch).unwrap();
    assert_eq!(initial_checkpoint.source(), epoch.source());
    assert_eq!(initial_checkpoint.build_id(), epoch.build_id());
    assert_eq!(initial_checkpoint.absolute_offset(), 0);

    let final_checkpoint = loop {
        match document.poll_candidate_recognition(epoch, 1).unwrap() {
            CandidateRecognitionPoll::NeedFuel(receipt) => {
                assert_eq!(receipt.work_units, 1);
                assert!(receipt.source_bytes_read <= 1);
            }
            CandidateRecognitionPoll::Atom {
                atom,
                checkpoint,
                receipt,
            } => {
                assert_eq!(receipt.work_units, 1);
                assert!(receipt.source_bytes_read <= 1);
                if matches!(
                    atom.kind(),
                    CandidateSourceAtomKind::LineEnding(CandidateLineEnding::CrLf)
                ) {
                    break checkpoint;
                }
            }
            CandidateRecognitionPoll::Eof(_) => panic!("line must end in CRLF"),
        }
    };
    assert_eq!(
        final_checkpoint.absolute_offset(),
        u64::try_from(source.len()).unwrap()
    );
    let recognition = document.candidate_finish_recognition_line(epoch).unwrap();
    assert_eq!(recognition.source(), epoch.source());
    assert_eq!(recognition.build_id(), epoch.build_id());
    assert_eq!(
        recognition.absolute_range(),
        (0, u64::try_from(source.len()).unwrap())
    );
    assert_eq!(recognition.ending(), Some(CandidateLineEnding::CrLf));
    assert_eq!(recognition.atom_count(), (REPEATS as u64) * 4 + 1);
    assert_eq!(
        document.poll_candidate_recognition(epoch, 1),
        Err(LiveDocumentError::SourceLedger(
            SourceBoundLedgerError::RecognitionReplayPending
        ))
    );

    let mut last_body_boundary = None;
    let line_ending = loop {
        match document.poll_candidate_source(epoch, 1).unwrap() {
            CandidateSourcePoll::NeedFuel(receipt) => {
                assert_eq!(receipt.work_units, 1);
                assert!(receipt.source_bytes_read <= 1);
            }
            CandidateSourcePoll::Atom { atom, receipt } => {
                assert_eq!(receipt.work_units, 1);
                assert!(receipt.source_bytes_read <= 1);
                if matches!(atom.kind(), CandidateSourceAtomKind::LineEnding(_)) {
                    break atom;
                }
                last_body_boundary = Some(atom.boundary());
            }
            CandidateSourcePoll::Eof(_) => panic!("authoritative replay lost CRLF"),
        }
    };
    let none = CandidateLogicalAction::none();
    document
        .candidate_claim_to(
            epoch,
            last_body_boundary.unwrap(),
            &paragraph,
            CoveragePart::BLOCK_MARKER,
            &none,
            GreenAffinity::Downstream,
        )
        .unwrap();
    document
        .candidate_claim_to(
            epoch,
            line_ending.boundary(),
            &paragraph,
            CoveragePart::TERMINAL,
            &none,
            GreenAffinity::Downstream,
        )
        .unwrap();
    let replay = document.candidate_finish_line(epoch).unwrap();
    assert!(replay.recognition_replay_matched());
    assert_eq!(replay.absolute_range(), recognition.absolute_range());
    assert_eq!(replay.metric(), recognition.metric());
    assert_eq!(replay.ending(), recognition.ending());
    assert_eq!(replay.atom_count(), recognition.atom_count());
    assert_eq!(replay.atom_debug_digest(), recognition.atom_debug_digest());

    assert!(matches!(
        document.poll_candidate_recognition(epoch, 1).unwrap(),
        CandidateRecognitionPoll::Eof(_)
    ));
    assert!(next_atom(&mut document, epoch, 1).is_none());
    document.candidate_close_binding(epoch, &paragraph).unwrap();
    document.candidate_close_binding(epoch, &root).unwrap();
    let seal = document.seal_candidate_source(epoch).unwrap();
    assert_eq!(seal.source_bytes_copied(), source.len());
    assert_eq!(seal.recognition_source_bytes_copied(), source.len());
    assert_eq!(seal.maximum_decoder_bytes(), 4);
    assert_eq!(seal.recognition_maximum_decoder_bytes(), 4);
    assert!(seal.maximum_source_chunk_bytes() <= 4 * 1024);
    assert!(seal.recognition_maximum_source_chunk_bytes() <= 4 * 1024);
    assert_eq!(
        seal.recognition_maximum_lead_bytes(),
        u64::try_from(source.len()).unwrap()
    );
    assert_eq!(seal.claim_count(), 2);
    assert_eq!(
        seal.authoritative_root_utf16(),
        u64::try_from(source.encode_utf16().count()).unwrap()
    );
}

#[test]
// The test keeps recognition, rejection, and authoritative replay together so
// no helper can accidentally make a recognition receipt look authoritative.
#[allow(clippy::too_many_lines)]
fn rejected_multiline_reference_candidate_replays_as_ordinary_paragraph_source() {
    // This is deliberately not a reference definition: neither the angle
    // destination nor the following title is closed. Recognition may still
    // need a bounded multi-line candidate range before the grammar rejects it.
    let source = "[broken]: <unterminated\n  \"still open\nplain 😀 tail";
    let (mut document, epoch) = activate(source);
    let (root, paragraph) = open_document_and_paragraph(&mut document, epoch);

    document
        .candidate_begin_recognition_range(
            epoch,
            CandidateRecognitionRangeKind::ReferenceDefinitionPrefix,
        )
        .unwrap();
    let mut recognized_ended_lines = 0_u64;
    loop {
        match document.poll_candidate_recognition(epoch, 1).unwrap() {
            CandidateRecognitionPoll::NeedFuel(receipt) => {
                assert_eq!(receipt.work_units, 1);
                assert!(receipt.source_bytes_read <= 1);
            }
            CandidateRecognitionPoll::Atom { atom, receipt, .. } => {
                assert_eq!(receipt.work_units, 1);
                assert!(receipt.source_bytes_read <= 1);
                if matches!(atom.kind(), CandidateSourceAtomKind::LineEnding(_)) {
                    let line = document
                        .candidate_continue_recognition_range_line(epoch)
                        .unwrap();
                    assert_eq!(line.line_ordinal(), recognized_ended_lines);
                    recognized_ended_lines += 1;
                }
            }
            CandidateRecognitionPoll::Eof(receipt) => {
                assert_eq!(receipt.work_units, 1);
                break;
            }
        }
    }
    assert_eq!(recognized_ended_lines, 2);
    let range = document.candidate_finish_recognition_range(epoch).unwrap();
    assert_eq!(range.source(), epoch.source());
    assert_eq!(range.build_id(), epoch.build_id());
    assert_eq!(
        range.kind(),
        CandidateRecognitionRangeKind::ReferenceDefinitionPrefix
    );
    assert_eq!(range.first_line(), 0);
    assert_eq!(range.line_count(), 3);
    assert_eq!(
        range.absolute_range(),
        (0, u64::try_from(source.len()).unwrap())
    );
    assert_eq!(range.metric().bytes(), u64::try_from(source.len()).unwrap());
    assert_eq!(
        range.metric().utf16(),
        u64::try_from(source.encode_utf16().count()).unwrap()
    );
    assert_eq!(
        document.poll_candidate_recognition(epoch, 1),
        Err(LiveDocumentError::SourceLedger(
            SourceBoundLedgerError::RecognitionReplayPending
        ))
    );

    let identity = CandidateLogicalAction::identity(&paragraph).unwrap();
    let none = CandidateLogicalAction::none();
    let mut replayed_lines = 0_u64;
    let mut last_body_boundary = None;
    loop {
        match document.poll_candidate_source(epoch, 1).unwrap() {
            CandidateSourcePoll::NeedFuel(receipt) => {
                assert_eq!(receipt.work_units, 1);
                assert!(receipt.source_bytes_read <= 1);
            }
            CandidateSourcePoll::Atom { atom, receipt } => {
                assert_eq!(receipt.work_units, 1);
                assert!(receipt.source_bytes_read <= 1);
                if matches!(atom.kind(), CandidateSourceAtomKind::LineEnding(_)) {
                    let content = document
                        .candidate_claim_to(
                            epoch,
                            last_body_boundary.take().unwrap(),
                            &paragraph,
                            CoveragePart::CONTENT,
                            &identity,
                            GreenAffinity::Downstream,
                        )
                        .unwrap();
                    assert_eq!(content.part(), CoveragePart::CONTENT);
                    assert_eq!(content.logical().kind(), ValidatedLogicalKind::Identity);
                    assert_eq!(content.owner_block(), paragraph.block_id());
                    let terminal = document
                        .candidate_claim_to(
                            epoch,
                            atom.boundary(),
                            &paragraph,
                            CoveragePart::TERMINAL,
                            &none,
                            GreenAffinity::Downstream,
                        )
                        .unwrap();
                    assert_eq!(terminal.part(), CoveragePart::TERMINAL);
                    assert_eq!(terminal.logical().kind(), ValidatedLogicalKind::None);
                    let line = document.candidate_finish_line(epoch).unwrap();
                    assert_eq!(line.line_ordinal(), replayed_lines);
                    assert!(!line.recognition_replay_matched());
                    replayed_lines += 1;
                } else {
                    last_body_boundary = Some(atom.boundary());
                }
            }
            CandidateSourcePoll::Eof(receipt) => {
                assert_eq!(receipt.work_units, 1);
                let content = document
                    .candidate_claim_to(
                        epoch,
                        last_body_boundary.take().unwrap(),
                        &paragraph,
                        CoveragePart::CONTENT,
                        &identity,
                        GreenAffinity::Downstream,
                    )
                    .unwrap();
                assert_eq!(content.part(), CoveragePart::CONTENT);
                assert_eq!(content.logical().kind(), ValidatedLogicalKind::Identity);
                assert_eq!(content.owner_block(), paragraph.block_id());
                let line = document.candidate_finish_line(epoch).unwrap();
                assert_eq!(line.line_ordinal(), replayed_lines);
                assert!(line.recognition_replay_matched());
                assert_eq!(line.absolute_range().1, range.absolute_range().1);
                replayed_lines += 1;
                break;
            }
        }
    }
    assert_eq!(replayed_lines, range.line_count());

    // The scanner-family receipt was never accepted back as authority. Only
    // fresh atoms from the authoritative replay minted the five exact claims.
    assert!(matches!(
        document.poll_candidate_recognition(epoch, 1).unwrap(),
        CandidateRecognitionPoll::Eof(_)
    ));
    document.candidate_close_binding(epoch, &paragraph).unwrap();
    document.candidate_close_binding(epoch, &root).unwrap();
    let seal = document.seal_candidate_source(epoch).unwrap();
    assert_eq!(seal.line_count(), 3);
    assert_eq!(seal.claim_count(), 5);
    assert_eq!(seal.source_bytes_copied(), source.len());
    assert_eq!(seal.recognition_source_bytes_copied(), source.len());
}
