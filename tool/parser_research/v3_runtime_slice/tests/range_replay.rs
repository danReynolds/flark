use std::convert::Infallible;

use flark_v3_runtime_slice::{
    CANDIDATE_RANGE_REPLAY_MAX_SOURCE_WORK_PER_POLL, CandidateRecognitionAtom,
    CandidateRecognitionSink, CandidateRecognitionWindowStatus, CandidateSourceAtomKind,
    CandidateWriterBinding, CandidateWriterConfig, CandidateWriterError, CandidateWriterProgress,
    CandidateWriterRangeRecipe, CandidateWriterSourcePoll, CoveragePart, FactsEnvelope,
    GrammarRevision, GreenAffinity, GreenKind, LiveCandidateEpoch, LiveDocumentStore,
    SourceBoundLedgerError,
};

const CONFIG: CandidateWriterConfig = CandidateWriterConfig {
    syntax_profile: 1,
    grammar_revision: GrammarRevision(1),
    semantic_epoch: 1,
};

fn activated_document(text: &str) -> (LiveDocumentStore, LiveCandidateEpoch) {
    let mut document = LiveDocumentStore::new(text, 8).unwrap();
    let token = document.active_parse_plan().unwrap().token;
    let epoch = document.begin_candidate(token).unwrap();
    document.activate_candidate_source_ledger(epoch).unwrap();
    document.activate_candidate_writer(epoch, CONFIG).unwrap();
    (document, epoch)
}

fn cancel(document: &mut LiveDocumentStore, epoch: LiveCandidateEpoch) {
    let abort = document.cancel_candidate(epoch).unwrap();
    while !document.poll_candidate_abort(abort, 64).unwrap().complete {}
}

#[derive(Debug, Default)]
struct DiscardRecognition {
    next_absolute_offset: u64,
}

impl CandidateRecognitionSink for DiscardRecognition {
    type Error = Infallible;

    fn push_recognition_atom(&mut self, atom: CandidateRecognitionAtom) -> Result<(), Self::Error> {
        let (start, end) = atom.absolute_range();
        assert_eq!(start, self.next_absolute_offset);
        self.next_absolute_offset = end;
        Ok(())
    }
}

fn recognize_line(document: &mut LiveDocumentStore, epoch: LiveCandidateEpoch) {
    let mut sink = DiscardRecognition::default();
    loop {
        let receipt = document
            .poll_candidate_writer_recognition_window(epoch, usize::MAX, &mut sink)
            .unwrap();
        match receipt.status() {
            CandidateRecognitionWindowStatus::BudgetExhausted => {}
            CandidateRecognitionWindowStatus::LineEnded(_)
            | CandidateRecognitionWindowStatus::Eof => {
                break;
            }
        }
    }
    let receipt = document
        .candidate_writer_finish_recognition_line(epoch)
        .unwrap();
    assert_eq!(receipt.absolute_range().1, sink.next_absolute_offset);
}

fn drive_writer(
    document: &mut LiveDocumentStore,
    epoch: LiveCandidateEpoch,
) -> CandidateWriterProgress {
    loop {
        match document.poll_candidate_writer(epoch).unwrap() {
            CandidateWriterProgress::Pending => {}
            complete => return complete,
        }
    }
}

fn open(
    document: &mut LiveDocumentStore,
    epoch: LiveCandidateEpoch,
    kind: GreenKind,
) -> CandidateWriterBinding {
    document
        .candidate_writer_start_open(epoch, kind, FactsEnvelope::empty())
        .unwrap();
    match drive_writer(document, epoch) {
        CandidateWriterProgress::Opened(binding) => binding,
        progress => panic!("open returned {progress:?}"),
    }
}

fn replay(
    document: &mut LiveDocumentStore,
    epoch: LiveCandidateEpoch,
    owner: &CandidateWriterBinding,
    part: CoveragePart,
    physical_bytes: u64,
    recipe: CandidateWriterRangeRecipe,
) -> flark_v3_runtime_slice::CandidateRangeReplayReceipt {
    document
        .candidate_writer_start_range_replay(epoch, owner, part, physical_bytes, recipe)
        .unwrap();
    match drive_writer(document, epoch) {
        CandidateWriterProgress::RangeReplayReady(receipt) => receipt,
        progress => panic!("range replay returned {progress:?}"),
    }
}

#[test]
fn sequential_exact_ranges_keep_the_ledger_authoritative_and_do_not_prefetch_eol() {
    let source = "aBCé\t\0\n";
    let (mut document, epoch) = activated_document(source);
    recognize_line(&mut document, epoch);
    let root = open(&mut document, epoch, GreenKind::DOCUMENT);
    let paragraph = open(&mut document, epoch, GreenKind::PARAGRAPH);

    let none = replay(
        &mut document,
        epoch,
        &root,
        CoveragePart::BLOCK_MARKER,
        1,
        CandidateWriterRangeRecipe::None,
    );
    let hidden = replay(
        &mut document,
        epoch,
        &paragraph,
        CoveragePart::CONTENT,
        1,
        CandidateWriterRangeRecipe::Hidden {
            affinity: GreenAffinity::Upstream,
        },
    );
    let identity = replay(
        &mut document,
        epoch,
        &paragraph,
        CoveragePart::CONTENT,
        1,
        CandidateWriterRangeRecipe::Identity,
    );
    let canonical = replay(
        &mut document,
        epoch,
        &paragraph,
        CoveragePart::CONTENT,
        4,
        CandidateWriterRangeRecipe::CanonicalText,
    );

    for receipt in [none, hidden, identity, canonical] {
        assert_eq!(receipt.source(), epoch.source());
        assert_eq!(receipt.build_id(), epoch.build_id());
        assert_eq!(receipt.line_ordinal(), 0);
    }
    assert_eq!(none.absolute_range(), (0, 1));
    assert_eq!(hidden.absolute_range(), (1, 2));
    assert_eq!(identity.absolute_range(), (2, 3));
    assert_eq!(canonical.absolute_range(), (3, 7));
    assert_eq!(canonical.physical_bytes(), 4);
    assert_eq!(canonical.physical_metric().bytes(), 4);
    assert_eq!(canonical.physical_metric().utf16(), 3);
    assert_eq!(canonical.source_work_units(), 4);
    assert_eq!(canonical.source_bytes_read(), 4);
    assert_eq!(canonical.atoms_scanned(), 3);
    assert_eq!(canonical.source_pieces(), 3);
    assert_eq!(canonical.maximum_pending_atoms(), 1);
    assert_eq!(canonical.maximum_pending_boundaries(), 1);

    match document.poll_candidate_writer_source(epoch, 1).unwrap() {
        CandidateWriterSourcePoll::Atom { atom, .. } => assert_eq!(
            atom.kind(),
            CandidateSourceAtomKind::LineEnding(flark_v3_runtime_slice::CandidateLineEnding::Lf)
        ),
        poll => panic!("exact range prefetched the line ending: {poll:?}"),
    }
    cancel(&mut document, epoch);
}

#[test]
fn split_utf8_endpoint_and_identity_over_tab_fail_closed_and_poison() {
    let (mut split, split_epoch) = activated_document("é\n");
    recognize_line(&mut split, split_epoch);
    let _root = open(&mut split, split_epoch, GreenKind::DOCUMENT);
    let paragraph = open(&mut split, split_epoch, GreenKind::PARAGRAPH);
    split
        .candidate_writer_start_range_replay(
            split_epoch,
            &paragraph,
            CoveragePart::CONTENT,
            1,
            CandidateWriterRangeRecipe::CanonicalText,
        )
        .unwrap();
    assert!(matches!(
        split.poll_candidate_writer(split_epoch),
        Err(CandidateWriterError::SourceLedger(
            SourceBoundLedgerError::RangeReplayEndpointSplitsAtom
        ))
    ));
    assert!(split.candidate_writer_is_poisoned(split_epoch).unwrap());
    cancel(&mut split, split_epoch);

    let (mut tab, tab_epoch) = activated_document("\t\n");
    recognize_line(&mut tab, tab_epoch);
    let _root = open(&mut tab, tab_epoch, GreenKind::DOCUMENT);
    let paragraph = open(&mut tab, tab_epoch, GreenKind::PARAGRAPH);
    tab.candidate_writer_start_range_replay(
        tab_epoch,
        &paragraph,
        CoveragePart::CONTENT,
        1,
        CandidateWriterRangeRecipe::Identity,
    )
    .unwrap();
    assert!(matches!(
        tab.poll_candidate_writer(tab_epoch),
        Err(CandidateWriterError::IdentityReplayRequiresTypedRecipe(
            CandidateSourceAtomKind::Tab
        ))
    ));
    assert!(tab.candidate_writer_is_poisoned(tab_epoch).unwrap());
    cancel(&mut tab, tab_epoch);
}

#[test]
fn endpoint_past_content_fails_at_plan_mint_and_only_cancellation_recovers() {
    let (mut document, epoch) = activated_document("x\r\n");
    recognize_line(&mut document, epoch);
    let _root = open(&mut document, epoch, GreenKind::DOCUMENT);
    let paragraph = open(&mut document, epoch, GreenKind::PARAGRAPH);

    assert_eq!(
        document.candidate_writer_start_range_replay(
            epoch,
            &paragraph,
            CoveragePart::CONTENT,
            2,
            CandidateWriterRangeRecipe::CanonicalText,
        ),
        Err(CandidateWriterError::SourceLedger(
            SourceBoundLedgerError::RangeReplayEndpointOutsideLine
        ))
    );
    assert!(document.candidate_writer_is_poisoned(epoch).unwrap());
    assert_eq!(
        document.candidate_writer_start_range_replay(
            epoch,
            &paragraph,
            CoveragePart::CONTENT,
            1,
            CandidateWriterRangeRecipe::CanonicalText,
        ),
        Err(CandidateWriterError::WriterPoisoned)
    );
    cancel(&mut document, epoch);
}

#[test]
fn only_terminal_none_may_claim_the_recognized_line_ending() {
    for part in [CoveragePart::BLOCK_MARKER, CoveragePart::GAP] {
        let (mut nonterminal, epoch) = activated_document("x\r\n");
        recognize_line(&mut nonterminal, epoch);
        let root = open(&mut nonterminal, epoch, GreenKind::DOCUMENT);

        assert_eq!(
            nonterminal.candidate_writer_start_range_replay(
                epoch,
                &root,
                part,
                3,
                CandidateWriterRangeRecipe::None,
            ),
            Err(CandidateWriterError::SourceLedger(
                SourceBoundLedgerError::RangeReplayEndpointOutsideLine
            ))
        );
        assert!(nonterminal.candidate_writer_is_poisoned(epoch).unwrap());
        cancel(&mut nonterminal, epoch);
    }

    let (mut terminal, terminal_epoch) = activated_document("x\r\n");
    recognize_line(&mut terminal, terminal_epoch);
    let root = open(&mut terminal, terminal_epoch, GreenKind::DOCUMENT);
    let receipt = replay(
        &mut terminal,
        terminal_epoch,
        &root,
        CoveragePart::TERMINAL,
        3,
        CandidateWriterRangeRecipe::None,
    );
    assert_eq!(receipt.absolute_range(), (0, 3));
    assert_eq!(receipt.physical_bytes(), 3);
    cancel(&mut terminal, terminal_epoch);
}

#[test]
fn active_range_is_linear_busy_and_cancellable_mid_scan() {
    let source = format!("{}\n", "a".repeat(1024 * 1024));
    let (mut document, epoch) = activated_document(&source);
    recognize_line(&mut document, epoch);
    let _root = open(&mut document, epoch, GreenKind::DOCUMENT);
    let paragraph = open(&mut document, epoch, GreenKind::PARAGRAPH);
    document
        .candidate_writer_start_range_replay(
            epoch,
            &paragraph,
            CoveragePart::CONTENT,
            1024 * 1024,
            CandidateWriterRangeRecipe::CanonicalText,
        )
        .unwrap();
    assert_eq!(
        document.candidate_writer_start_range_replay(
            epoch,
            &paragraph,
            CoveragePart::CONTENT,
            1,
            CandidateWriterRangeRecipe::None,
        ),
        Err(CandidateWriterError::Busy)
    );
    assert!(matches!(
        document.poll_candidate_writer(epoch).unwrap(),
        CandidateWriterProgress::Pending
    ));
    cancel(&mut document, epoch);
}

#[test]
fn cancellation_releases_range_authority_from_drain_and_ready_phases() {
    // One scalar reaches its endpoint in the first poll and installs the
    // composer Drain phase before yielding.
    let (mut draining, draining_epoch) = activated_document("x");
    recognize_line(&mut draining, draining_epoch);
    let _root = open(&mut draining, draining_epoch, GreenKind::DOCUMENT);
    let paragraph = open(&mut draining, draining_epoch, GreenKind::PARAGRAPH);
    draining
        .candidate_writer_start_range_replay(
            draining_epoch,
            &paragraph,
            CoveragePart::CONTENT,
            1,
            CandidateWriterRangeRecipe::CanonicalText,
        )
        .unwrap();
    assert!(matches!(
        draining.poll_candidate_writer(draining_epoch).unwrap(),
        CandidateWriterProgress::Pending
    ));
    cancel(&mut draining, draining_epoch);

    // The following poll completes that drain and installs Ready; the public
    // receipt remains owned by the still-active job until one further poll.
    let (mut ready, ready_epoch) = activated_document("x");
    recognize_line(&mut ready, ready_epoch);
    let _root = open(&mut ready, ready_epoch, GreenKind::DOCUMENT);
    let paragraph = open(&mut ready, ready_epoch, GreenKind::PARAGRAPH);
    ready
        .candidate_writer_start_range_replay(
            ready_epoch,
            &paragraph,
            CoveragePart::CONTENT,
            1,
            CandidateWriterRangeRecipe::CanonicalText,
        )
        .unwrap();
    for _ in 0..2 {
        assert!(matches!(
            ready.poll_candidate_writer(ready_epoch).unwrap(),
            CandidateWriterProgress::Pending
        ));
    }
    cancel(&mut ready, ready_epoch);
}

#[test]
fn ten_mib_sparse_typed_range_has_linear_source_work_and_constant_pending_state() {
    const BODY_BYTES: usize = 10 * 1024 * 1024;
    const CHUNKS: usize = 10;
    const CHUNK_BYTES: usize = BODY_BYTES / CHUNKS;
    let mut source = String::with_capacity(BODY_BYTES + 1);
    for _ in 0..CHUNKS {
        source.push_str(&"a".repeat(CHUNK_BYTES - 2));
        source.push('\t');
        source.push('\0');
    }
    assert_eq!(source.len(), BODY_BYTES);
    source.push('\n');

    let (mut document, epoch) = activated_document(&source);
    recognize_line(&mut document, epoch);
    let _root = open(&mut document, epoch, GreenKind::DOCUMENT);
    let paragraph = open(&mut document, epoch, GreenKind::PARAGRAPH);
    let receipt = replay(
        &mut document,
        epoch,
        &paragraph,
        CoveragePart::CONTENT,
        u64::try_from(BODY_BYTES).unwrap(),
        CandidateWriterRangeRecipe::CanonicalText,
    );

    assert_eq!(receipt.absolute_range(), (0, BODY_BYTES as u64));
    assert_eq!(receipt.physical_metric().bytes(), BODY_BYTES as u64);
    assert_eq!(receipt.physical_metric().utf16(), BODY_BYTES as u64);
    assert_eq!(receipt.source_work_units(), BODY_BYTES as u64);
    assert_eq!(receipt.source_bytes_read(), BODY_BYTES as u64);
    assert_eq!(receipt.atoms_scanned(), BODY_BYTES as u64);
    assert_eq!(receipt.source_pieces(), u64::try_from(CHUNKS * 3).unwrap());
    assert_eq!(receipt.maximum_pending_atoms(), 1);
    assert_eq!(receipt.maximum_pending_boundaries(), 1);
    assert!(
        receipt.writer_polls()
            >= (BODY_BYTES as u64).div_ceil(CANDIDATE_RANGE_REPLAY_MAX_SOURCE_WORK_PER_POLL as u64)
    );

    match document.poll_candidate_writer_source(epoch, 1).unwrap() {
        CandidateWriterSourcePoll::Atom { atom, .. } => assert!(matches!(
            atom.kind(),
            CandidateSourceAtomKind::LineEnding(_)
        )),
        poll => panic!("10 MiB range prefetched its terminator: {poll:?}"),
    }
    cancel(&mut document, epoch);
}
