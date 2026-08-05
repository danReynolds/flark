use std::convert::Infallible;

use flark_v3_runtime_slice::{
    CANDIDATE_RECOGNITION_WINDOW_MAX_WORK, CandidateLineEnding, CandidateRecognitionAtom,
    CandidateRecognitionSink, CandidateRecognitionWindowError, CandidateRecognitionWindowStatus,
    CandidateWriterConfig, CandidateWriterError, GrammarRevision, LiveCandidateEpoch,
    LiveDocumentError, LiveDocumentStore,
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
struct CountingSink {
    atoms: usize,
    bytes: u64,
    utf16: u64,
    next_absolute_offset: u64,
    endings: usize,
}

impl CandidateRecognitionSink for CountingSink {
    type Error = Infallible;

    fn push_recognition_atom(&mut self, atom: CandidateRecognitionAtom) -> Result<(), Self::Error> {
        let (start, end) = atom.absolute_range();
        assert_eq!(start, self.next_absolute_offset);
        self.next_absolute_offset = end;
        self.atoms += 1;
        self.bytes += end - start;
        self.utf16 += match atom.kind() {
            flark_v3_runtime_slice::CandidateSourceAtomKind::Scalar(scalar) => {
                u64::try_from(scalar.len_utf16()).unwrap()
            }
            flark_v3_runtime_slice::CandidateSourceAtomKind::Tab
            | flark_v3_runtime_slice::CandidateSourceAtomKind::Nul => 1,
            flark_v3_runtime_slice::CandidateSourceAtomKind::LineEnding(ending) => {
                self.endings += 1;
                match ending {
                    CandidateLineEnding::Lf | CandidateLineEnding::LoneCr => 1,
                    CandidateLineEnding::CrLf => 2,
                }
            }
        };
        Ok(())
    }
}

#[derive(Debug, Default)]
struct WindowTotals {
    polls: usize,
    work: usize,
    source_bytes_read: usize,
}

fn recognize_one_line(
    document: &mut LiveDocumentStore,
    epoch: LiveCandidateEpoch,
    sink: &mut CountingSink,
) -> (CandidateRecognitionWindowStatus, WindowTotals) {
    let mut expected_start = 0_u64;
    let mut totals = WindowTotals::default();
    loop {
        let receipt = document
            .poll_candidate_writer_recognition_window(epoch, usize::MAX, sink)
            .unwrap();
        let work = receipt.work();
        assert!(work.work_units <= CANDIDATE_RECOGNITION_WINDOW_MAX_WORK);
        assert!(work.source_bytes_read <= work.work_units);
        assert_eq!(receipt.start().source(), epoch.source());
        assert_eq!(receipt.end().source(), epoch.source());
        assert_eq!(receipt.start().build_id(), epoch.build_id());
        assert_eq!(receipt.end().build_id(), epoch.build_id());
        assert_eq!(receipt.start().absolute_offset(), expected_start);
        expected_start = receipt.end().absolute_offset();
        totals.polls += 1;
        totals.work += work.work_units;
        totals.source_bytes_read += work.source_bytes_read;

        match receipt.status() {
            CandidateRecognitionWindowStatus::BudgetExhausted => {}
            terminal @ (CandidateRecognitionWindowStatus::LineEnded(_)
            | CandidateRecognitionWindowStatus::Eof) => return (terminal, totals),
        }
    }
}

#[test]
fn zero_fuel_is_no_work_and_foreign_source_epoch_never_reaches_the_sink() {
    let (mut document, epoch) = activated_document("abc\n");
    let (_foreign_document, foreign_epoch) = activated_document("different source\n");
    assert_ne!(epoch.source(), foreign_epoch.source());

    let mut sink = CountingSink::default();
    assert_eq!(
        document.poll_candidate_writer_recognition_window(foreign_epoch, 4096, &mut sink),
        Err(CandidateRecognitionWindowError::Infrastructure(
            CandidateWriterError::Actor(LiveDocumentError::WrongCandidateEpoch)
        ))
    );
    assert_eq!(sink.atoms, 0);
    assert!(!document.candidate_writer_is_poisoned(epoch).unwrap());

    let before = document
        .candidate_writer_recognition_checkpoint(epoch)
        .unwrap();
    let receipt = document
        .poll_candidate_writer_recognition_window(epoch, 0, &mut sink)
        .unwrap();
    assert_eq!(
        receipt.status(),
        CandidateRecognitionWindowStatus::BudgetExhausted
    );
    assert_eq!(receipt.work().work_units, 0);
    assert_eq!(receipt.work().source_bytes_read, 0);
    assert_eq!(receipt.atoms_emitted(), 0);
    assert_eq!(receipt.start(), before);
    assert_eq!(receipt.end(), before);
    assert_eq!(sink.atoms, 0);
    assert!(!document.candidate_writer_is_poisoned(epoch).unwrap());
    cancel(&mut document, epoch);
}

#[test]
fn window_is_hard_capped_at_four_kib_and_stops_at_the_first_line_boundary() {
    let first_line_body = "a".repeat(CANDIDATE_RECOGNITION_WINDOW_MAX_WORK * 2 + 1);
    let source = format!("{first_line_body}\nNEXT LINE");
    let first_line_end = u64::try_from(first_line_body.len() + 1).unwrap();
    let (mut document, epoch) = activated_document(&source);
    let mut sink = CountingSink::default();

    let first = document
        .poll_candidate_writer_recognition_window(epoch, usize::MAX, &mut sink)
        .unwrap();
    assert_eq!(
        first.work().work_units,
        CANDIDATE_RECOGNITION_WINDOW_MAX_WORK
    );
    assert_eq!(
        first.status(),
        CandidateRecognitionWindowStatus::BudgetExhausted
    );

    let second = document
        .poll_candidate_writer_recognition_window(epoch, usize::MAX, &mut sink)
        .unwrap();
    assert_eq!(
        second.work().work_units,
        CANDIDATE_RECOGNITION_WINDOW_MAX_WORK
    );
    assert_eq!(
        second.status(),
        CandidateRecognitionWindowStatus::BudgetExhausted
    );

    let terminal = document
        .poll_candidate_writer_recognition_window(epoch, usize::MAX, &mut sink)
        .unwrap();
    assert_eq!(
        terminal.status(),
        CandidateRecognitionWindowStatus::LineEnded(CandidateLineEnding::Lf)
    );
    assert_eq!(terminal.end().absolute_offset(), first_line_end);
    assert_eq!(sink.next_absolute_offset, first_line_end);
    assert_eq!(sink.endings, 1);
    assert_eq!(sink.atoms, first_line_body.len() + 1);

    let line = document
        .candidate_writer_finish_recognition_line(epoch)
        .unwrap();
    assert_eq!(line.absolute_range(), (0, first_line_end));
    assert_eq!(line.metric().bytes(), first_line_end);
    assert_eq!(line.metric().utf16(), first_line_end);
    assert_eq!(line.atom_count(), u64::try_from(sink.atoms).unwrap());
    cancel(&mut document, epoch);
}

#[test]
fn ten_mib_ascii_line_is_streamed_with_bounded_receipts() {
    const BODY_BYTES: usize = 10 * 1024 * 1024;
    let mut source = "a".repeat(BODY_BYTES);
    source.push_str("\r\n");
    let expected_bytes = u64::try_from(source.len()).unwrap();
    let (mut document, epoch) = activated_document(&source);
    let mut sink = CountingSink::default();

    let (status, totals) = recognize_one_line(&mut document, epoch, &mut sink);
    assert_eq!(
        status,
        CandidateRecognitionWindowStatus::LineEnded(CandidateLineEnding::CrLf)
    );
    assert_eq!(sink.bytes, expected_bytes);
    assert_eq!(sink.utf16, expected_bytes);
    assert_eq!(sink.atoms, BODY_BYTES + 1);
    assert_eq!(sink.endings, 1);
    assert_eq!(totals.source_bytes_read, source.len());
    assert_eq!(totals.work, source.len());
    assert!(totals.polls <= source.len().div_ceil(CANDIDATE_RECOGNITION_WINDOW_MAX_WORK));

    let line = document
        .candidate_writer_finish_recognition_line(epoch)
        .unwrap();
    assert_eq!(line.absolute_range(), (0, expected_bytes));
    assert_eq!(line.metric().bytes(), expected_bytes);
    assert_eq!(line.metric().utf16(), expected_bytes);
    assert_eq!(line.atom_count(), u64::try_from(BODY_BYTES + 1).unwrap());
    cancel(&mut document, epoch);
}

#[test]
fn ten_mib_mixed_width_unicode_bare_eof_preserves_exact_byte_and_utf16_metrics() {
    const BODY_BYTES: usize = 10 * 1024 * 1024;
    const PATTERN: &str = "aé😀β";
    const PATTERN_BYTES: usize = 9;
    const PATTERN_ATOMS: usize = 4;
    const PATTERN_UTF16: u64 = 5;

    let repeats = BODY_BYTES / PATTERN_BYTES;
    let ascii_tail = BODY_BYTES % PATTERN_BYTES;
    let mut source = PATTERN.repeat(repeats);
    source.push_str(&"x".repeat(ascii_tail));
    assert_eq!(source.len(), BODY_BYTES);
    let expected_atoms = repeats * PATTERN_ATOMS + ascii_tail;
    let expected_utf16 =
        u64::try_from(repeats).unwrap() * PATTERN_UTF16 + u64::try_from(ascii_tail).unwrap();

    let (mut document, epoch) = activated_document(&source);
    let mut sink = CountingSink::default();
    let (status, totals) = recognize_one_line(&mut document, epoch, &mut sink);
    assert_eq!(status, CandidateRecognitionWindowStatus::Eof);
    assert_eq!(sink.bytes, u64::try_from(BODY_BYTES).unwrap());
    assert_eq!(sink.utf16, expected_utf16);
    assert_eq!(sink.atoms, expected_atoms);
    assert_eq!(sink.endings, 0);
    assert_eq!(totals.source_bytes_read, BODY_BYTES);
    assert_eq!(totals.work, BODY_BYTES + 1);
    assert!(totals.polls <= (BODY_BYTES + 1).div_ceil(CANDIDATE_RECOGNITION_WINDOW_MAX_WORK));

    let line = document
        .candidate_writer_finish_recognition_line(epoch)
        .unwrap();
    assert_eq!(
        line.absolute_range(),
        (0, u64::try_from(BODY_BYTES).unwrap())
    );
    assert_eq!(line.metric().bytes(), u64::try_from(BODY_BYTES).unwrap());
    assert_eq!(line.metric().utf16(), expected_utf16);
    assert_eq!(line.atom_count(), u64::try_from(expected_atoms).unwrap());
    cancel(&mut document, epoch);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InjectedSinkError {
    ThirdAtom,
}

#[derive(Debug, Default)]
struct FailingSink {
    calls: usize,
}

impl CandidateRecognitionSink for FailingSink {
    type Error = InjectedSinkError;

    fn push_recognition_atom(
        &mut self,
        _atom: CandidateRecognitionAtom,
    ) -> Result<(), Self::Error> {
        self.calls += 1;
        if self.calls == 3 {
            Err(InjectedSinkError::ThirdAtom)
        } else {
            Ok(())
        }
    }
}

#[test]
fn sink_failure_poisoning_forces_whole_candidate_cancellation() {
    let (mut document, epoch) = activated_document("abcdef\n");
    let mut sink = FailingSink::default();
    assert_eq!(
        document.poll_candidate_writer_recognition_window(epoch, 4096, &mut sink),
        Err(CandidateRecognitionWindowError::Sink(
            InjectedSinkError::ThirdAtom
        ))
    );
    assert_eq!(sink.calls, 3);
    assert!(document.candidate_writer_is_poisoned(epoch).unwrap());

    let mut replacement_sink = CountingSink::default();
    assert_eq!(
        document.poll_candidate_writer_recognition_window(epoch, 4096, &mut replacement_sink),
        Err(CandidateRecognitionWindowError::Infrastructure(
            CandidateWriterError::WriterPoisoned
        ))
    );
    assert_eq!(replacement_sink.atoms, 0);

    cancel(&mut document, epoch);
    assert_eq!(
        document.candidate_writer_is_poisoned(epoch),
        Err(CandidateWriterError::Actor(LiveDocumentError::NoCandidate))
    );
}
