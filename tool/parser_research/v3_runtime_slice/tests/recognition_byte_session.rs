use std::convert::Infallible;

use flark_v3_runtime_slice::{
    CANDIDATE_RECOGNITION_BYTE_POLL_MAX_ACCESSES, CandidateLineEnding,
    CandidateRecognitionByteAccessError, CandidateRecognitionBytePollError,
    CandidateRecognitionByteScanner, CandidateRecognitionByteSession,
    CandidateRecognitionByteSource, CandidateWriterConfig, CandidateWriterError, GrammarRevision,
    LiveCandidateEpoch, LiveDocumentError, LiveDocumentStore, SourceBoundLedgerError,
    SourcePhysicalLineEnding,
};

const CONFIG: CandidateWriterConfig = CandidateWriterConfig {
    syntax_profile: 1,
    grammar_revision: GrammarRevision(1),
    semantic_epoch: 1,
};

const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

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

fn hash_bytes(bytes: &[u8]) -> u64 {
    bytes.iter().fold(FNV_OFFSET_BASIS, |mut digest, byte| {
        digest ^= u64::from(*byte);
        digest.wrapping_mul(FNV_PRIME)
    })
}

#[derive(Debug)]
struct SequentialScanner {
    next: usize,
    digest: u64,
}

impl Default for SequentialScanner {
    fn default() -> Self {
        Self {
            next: 0,
            digest: FNV_OFFSET_BASIS,
        }
    }
}

impl CandidateRecognitionByteScanner for SequentialScanner {
    type Error = CandidateRecognitionByteAccessError;

    fn poll(&mut self, source: &mut CandidateRecognitionByteSource<'_>) -> Result<(), Self::Error> {
        while self.next < source.len() {
            match source.byte_at(self.next) {
                Ok(byte) => {
                    self.digest ^= u64::from(byte);
                    self.digest = self.digest.wrapping_mul(FNV_PRIME);
                    self.next += 1;
                }
                Err(CandidateRecognitionByteAccessError::BudgetExhausted) => return Ok(()),
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }
}

#[derive(Debug, Default)]
struct DrainTotals {
    polls: usize,
    accesses: usize,
    new_bytes: usize,
    source_bytes_read: usize,
    decoded_atoms: usize,
}

fn drain_session(
    document: &mut LiveDocumentStore,
    epoch: LiveCandidateEpoch,
    session: CandidateRecognitionByteSession,
    scanner: &mut SequentialScanner,
) -> DrainTotals {
    let mut totals = DrainTotals::default();
    let mut expected_high_water = 0;
    while scanner.next < session.len() {
        let receipt = document
            .poll_candidate_writer_recognition_byte_session(epoch, session, usize::MAX, scanner)
            .unwrap();
        assert_eq!(receipt.session(), session);
        assert_eq!(receipt.exposed_high_water().0, expected_high_water);
        expected_high_water = receipt.exposed_high_water().1;
        assert_eq!(expected_high_water, scanner.next);
        assert_eq!(
            receipt.physical_high_water(),
            u64::try_from(session.start() + expected_high_water).unwrap()
        );
        assert!(receipt.access_work_units() <= CANDIDATE_RECOGNITION_BYTE_POLL_MAX_ACCESSES);
        assert_eq!(receipt.access_work_units(), receipt.new_bytes());
        assert_eq!(receipt.source_bytes_read(), receipt.new_bytes());
        assert_eq!(receipt.repeated_last_byte_peeks(), 0);
        assert!(receipt.maximum_retained_byte_scratch() <= 1);
        assert_eq!(
            receipt.budget_exhausted(),
            expected_high_water < session.len()
        );
        totals.polls += 1;
        totals.accesses += receipt.access_work_units();
        totals.new_bytes += receipt.new_bytes();
        totals.source_bytes_read += receipt.source_bytes_read();
        totals.decoded_atoms += receipt.decoded_atoms();
    }
    assert_eq!(expected_high_water, session.len());
    totals
}

#[test]
fn ten_mib_ascii_crlf_is_exposed_with_bounded_work_and_one_byte_scratch() {
    const BODY_BYTES: usize = 10 * 1024 * 1024;
    let mut source = "a".repeat(BODY_BYTES);
    source.push_str("\r\n");
    let (mut document, epoch) = activated_document(&source);
    let descriptor = document
        .candidate_writer_recognition_line_descriptor(epoch)
        .unwrap();
    assert_eq!(descriptor.epoch(), epoch);
    assert_eq!(descriptor.physical_utf16(), source.len());
    let session = document
        .candidate_writer_begin_recognition_byte_session(epoch, descriptor)
        .unwrap();
    assert_eq!(session.len(), source.len());
    assert_eq!(session.ending(), SourcePhysicalLineEnding::CrLf);

    let mut scanner = SequentialScanner::default();
    let totals = drain_session(&mut document, epoch, session, &mut scanner);
    assert_eq!(scanner.digest, hash_bytes(source.as_bytes()));
    assert_eq!(totals.new_bytes, source.len());
    assert_eq!(totals.source_bytes_read, source.len());
    assert_eq!(totals.decoded_atoms, BODY_BYTES + 1);
    assert!(
        totals.polls
            <= source
                .len()
                .div_ceil(CANDIDATE_RECOGNITION_BYTE_POLL_MAX_ACCESSES)
    );

    let finished = document
        .candidate_writer_finish_recognition_byte_session(epoch, session)
        .unwrap();
    assert_eq!(finished.line().ending(), Some(CandidateLineEnding::CrLf));
    assert_eq!(
        finished.line().metric().bytes(),
        u64::try_from(source.len()).unwrap()
    );
    assert_eq!(
        finished.line().metric().utf16(),
        u64::try_from(source.len()).unwrap()
    );
    assert_eq!(finished.new_bytes(), u64::try_from(source.len()).unwrap());
    assert_eq!(
        finished.source_bytes_read(),
        u64::try_from(source.len()).unwrap()
    );
    assert_eq!(finished.repeated_last_byte_peeks(), 0);
    assert_eq!(
        finished.decoded_atoms(),
        u64::try_from(BODY_BYTES + 1).unwrap()
    );
    assert_eq!(
        finished.physical_high_water(),
        u64::try_from(source.len()).unwrap()
    );
    assert_eq!(finished.maximum_retained_byte_scratch(), 1);
    cancel(&mut document, epoch);
}

#[test]
fn ten_mib_mixed_unicode_bare_eof_preserves_indexed_utf16_exactly() {
    const BODY_BYTES: usize = 10 * 1024 * 1024;
    const PATTERN: &str = "aé😀β";
    const PATTERN_BYTES: usize = 9;
    const PATTERN_ATOMS: usize = 4;
    const PATTERN_UTF16: usize = 5;

    let repeats = BODY_BYTES / PATTERN_BYTES;
    let ascii_tail = BODY_BYTES % PATTERN_BYTES;
    let mut source = PATTERN.repeat(repeats);
    source.push_str(&"x".repeat(ascii_tail));
    let expected_utf16 = repeats * PATTERN_UTF16 + ascii_tail;
    let expected_atoms = repeats * PATTERN_ATOMS + ascii_tail;
    assert_eq!(source.len(), BODY_BYTES);

    let (mut document, epoch) = activated_document(&source);
    let descriptor = document
        .candidate_writer_recognition_line_descriptor(epoch)
        .unwrap();
    assert_eq!(descriptor.physical_utf16(), expected_utf16);
    let session = document
        .candidate_writer_begin_recognition_byte_session(epoch, descriptor)
        .unwrap();
    assert_eq!(session.ending(), SourcePhysicalLineEnding::BareEof);
    assert_eq!(session.physical_utf16(), expected_utf16);

    let mut scanner = SequentialScanner::default();
    let totals = drain_session(&mut document, epoch, session, &mut scanner);
    assert_eq!(scanner.digest, hash_bytes(source.as_bytes()));
    assert_eq!(totals.new_bytes, BODY_BYTES);
    assert_eq!(totals.source_bytes_read, BODY_BYTES);
    assert_eq!(totals.decoded_atoms, expected_atoms);

    let finished = document
        .candidate_writer_finish_recognition_byte_session(epoch, session)
        .unwrap();
    assert_eq!(finished.line().ending(), None);
    assert_eq!(
        finished.line().metric().bytes(),
        u64::try_from(BODY_BYTES).unwrap()
    );
    assert_eq!(
        finished.line().metric().utf16(),
        u64::try_from(expected_utf16).unwrap()
    );
    assert_eq!(
        finished.line().atom_count(),
        u64::try_from(expected_atoms).unwrap()
    );
    assert_eq!(
        finished.physical_high_water(),
        u64::try_from(BODY_BYTES).unwrap()
    );
    assert_eq!(finished.maximum_retained_byte_scratch(), 1);
    cancel(&mut document, epoch);
}

#[test]
fn indexed_lf_crlf_lone_cr_and_bare_eof_bounds_are_exact() {
    let cases = [
        ("a\nnext", SourcePhysicalLineEnding::Lf, 2, 2),
        ("a\r\nnext", SourcePhysicalLineEnding::CrLf, 3, 3),
        ("a\rnext", SourcePhysicalLineEnding::LoneCr, 2, 2),
        ("é😀", SourcePhysicalLineEnding::BareEof, 6, 3),
    ];

    for (source, ending, physical_bytes, physical_utf16) in cases {
        let (mut document, epoch) = activated_document(source);
        let descriptor = document
            .candidate_writer_recognition_line_descriptor(epoch)
            .unwrap();
        assert_eq!(descriptor.end(), physical_bytes);
        assert_eq!(descriptor.physical_utf16(), physical_utf16);
        let session = document
            .candidate_writer_begin_recognition_byte_session(epoch, descriptor)
            .unwrap();
        assert_eq!(session.ending(), ending);
        let mut scanner = SequentialScanner::default();
        let totals = drain_session(&mut document, epoch, session, &mut scanner);
        assert_eq!(totals.source_bytes_read, physical_bytes);
        assert_eq!(
            scanner.digest,
            hash_bytes(&source.as_bytes()[..physical_bytes])
        );
        let finished = document
            .candidate_writer_finish_recognition_byte_session(epoch, session)
            .unwrap();
        assert_eq!(
            finished.physical_high_water(),
            u64::try_from(physical_bytes).unwrap()
        );
        assert_eq!(
            finished.line().metric().utf16(),
            u64::try_from(physical_utf16).unwrap()
        );
        cancel(&mut document, epoch);
    }
}

#[test]
fn only_the_last_byte_may_repeat_and_repeats_do_not_refeed_the_decoder() {
    let (mut document, epoch) = activated_document("ab\n");
    let descriptor = document
        .candidate_writer_recognition_line_descriptor(epoch)
        .unwrap();
    let session = document
        .candidate_writer_begin_recognition_byte_session(epoch, descriptor)
        .unwrap();
    let mut scanner = |source: &mut CandidateRecognitionByteSource<'_>| {
        for (offset, expected) in [(0, b'a'), (1, b'b'), (2, b'\n')] {
            assert_eq!(source.byte_at(offset).unwrap(), expected);
            assert_eq!(source.byte_at(offset).unwrap(), expected);
        }
        Ok::<_, Infallible>(())
    };
    let receipt = document
        .poll_candidate_writer_recognition_byte_session(epoch, session, usize::MAX, &mut scanner)
        .unwrap();
    assert_eq!(receipt.access_work_units(), 6);
    assert_eq!(receipt.new_bytes(), 3);
    assert_eq!(receipt.source_bytes_read(), 3);
    assert_eq!(receipt.repeated_last_byte_peeks(), 3);
    assert_eq!(receipt.decoded_atoms(), 3);
    assert_eq!(receipt.maximum_retained_byte_scratch(), 1);

    let finished = document
        .candidate_writer_finish_recognition_byte_session(epoch, session)
        .unwrap();
    assert_eq!(finished.total_access_work_units(), 6);
    assert_eq!(finished.new_bytes(), 3);
    assert_eq!(finished.source_bytes_read(), 3);
    assert_eq!(finished.repeated_last_byte_peeks(), 3);
    assert_eq!(finished.decoded_atoms(), 3);
    cancel(&mut document, epoch);
}

#[test]
fn out_of_order_and_past_bound_access_fail_closed_and_poison_the_candidate() {
    for past_bound in [false, true] {
        let (mut document, epoch) = activated_document("ab\n");
        let descriptor = document
            .candidate_writer_recognition_line_descriptor(epoch)
            .unwrap();
        let session = document
            .candidate_writer_begin_recognition_byte_session(epoch, descriptor)
            .unwrap();
        let mut scanner = |source: &mut CandidateRecognitionByteSource<'_>| {
            let error = if past_bound {
                source.byte_at(source.len()).unwrap_err()
            } else {
                source.byte_at(1).unwrap_err()
            };
            if past_bound {
                assert_eq!(
                    error,
                    CandidateRecognitionByteAccessError::LogicalEof {
                        requested: source.len(),
                        len: source.len(),
                    }
                );
            } else {
                assert_eq!(
                    error,
                    CandidateRecognitionByteAccessError::OutOfOrder {
                        requested: 1,
                        next_sequential: 0,
                    }
                );
            }
            Ok::<_, Infallible>(())
        };
        let expected = if past_bound {
            SourceBoundLedgerError::RecognitionBytePastLine
        } else {
            SourceBoundLedgerError::RecognitionByteOutOfOrder
        };
        assert_eq!(
            document.poll_candidate_writer_recognition_byte_session(
                epoch,
                session,
                usize::MAX,
                &mut scanner,
            ),
            Err(CandidateRecognitionBytePollError::Infrastructure(
                CandidateWriterError::SourceLedger(expected)
            ))
        );
        assert!(document.candidate_writer_is_poisoned(epoch).unwrap());
        cancel(&mut document, epoch);
    }
}

#[test]
fn foreign_descriptor_and_session_fail_at_the_actor_without_poisoning_local_state() {
    let (mut local, local_epoch) = activated_document("local\n");
    let (mut foreign, foreign_epoch) = activated_document("foreign\n");
    let foreign_descriptor = foreign
        .candidate_writer_recognition_line_descriptor(foreign_epoch)
        .unwrap();
    assert_eq!(
        local.candidate_writer_begin_recognition_byte_session(local_epoch, foreign_descriptor),
        Err(CandidateWriterError::Actor(
            LiveDocumentError::WrongCandidateEpoch
        ))
    );
    assert!(!local.candidate_writer_is_poisoned(local_epoch).unwrap());

    let foreign_session = foreign
        .candidate_writer_begin_recognition_byte_session(foreign_epoch, foreign_descriptor)
        .unwrap();
    let mut unreachable = |_source: &mut CandidateRecognitionByteSource<'_>| {
        panic!("foreign session reached the local scanner");
        #[allow(unreachable_code)]
        Ok::<_, Infallible>(())
    };
    assert_eq!(
        local.poll_candidate_writer_recognition_byte_session(
            local_epoch,
            foreign_session,
            usize::MAX,
            &mut unreachable,
        ),
        Err(CandidateRecognitionBytePollError::Infrastructure(
            CandidateWriterError::Actor(LiveDocumentError::WrongCandidateEpoch)
        ))
    );
    assert_eq!(
        local.candidate_writer_finish_recognition_byte_session(local_epoch, foreign_session),
        Err(CandidateWriterError::Actor(
            LiveDocumentError::WrongCandidateEpoch
        ))
    );
    assert!(!local.candidate_writer_is_poisoned(local_epoch).unwrap());
    cancel(&mut local, local_epoch);
    cancel(&mut foreign, foreign_epoch);
}

#[test]
fn empty_bare_eof_remains_queryable_but_cannot_begin_a_byte_session() {
    let (mut document, epoch) = activated_document("");
    let descriptor = document
        .candidate_writer_recognition_line_descriptor(epoch)
        .unwrap();
    assert_eq!(descriptor.start(), 0);
    assert_eq!(descriptor.end(), 0);
    assert_eq!(descriptor.physical_utf16(), 0);
    assert_eq!(
        descriptor.physical_line().ending(),
        SourcePhysicalLineEnding::BareEof
    );
    assert_eq!(
        document.candidate_writer_begin_recognition_byte_session(epoch, descriptor),
        Err(CandidateWriterError::SourceLedger(
            SourceBoundLedgerError::RecognitionByteEmptyBareEof
        ))
    );
    assert!(document.candidate_writer_is_poisoned(epoch).unwrap());
    cancel(&mut document, epoch);
}

#[test]
fn an_open_byte_session_excludes_scalar_recognition_and_authoritative_reads() {
    let (mut document, epoch) = activated_document("abc\n");
    let descriptor = document
        .candidate_writer_recognition_line_descriptor(epoch)
        .unwrap();
    let _session = document
        .candidate_writer_begin_recognition_byte_session(epoch, descriptor)
        .unwrap();
    assert_eq!(
        document.candidate_writer_recognition_line_descriptor(epoch),
        Err(CandidateWriterError::SourceLedger(
            SourceBoundLedgerError::RecognitionByteSessionAlreadyOpen
        ))
    );
    assert!(!document.candidate_writer_is_poisoned(epoch).unwrap());
    assert!(matches!(
        document.poll_candidate_writer_source(epoch, 1),
        Err(CandidateWriterError::SourceLedger(
            SourceBoundLedgerError::RecognitionByteSessionAlreadyOpen
        ))
    ));
    assert!(document.candidate_writer_is_poisoned(epoch).unwrap());
    cancel(&mut document, epoch);
}
