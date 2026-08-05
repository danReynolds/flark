use flark_v3_runtime_slice::{
    CandidateWriterConfig, CandidateWriterError, GrammarRevision, LiveCandidateEpoch,
    LiveDocumentError, LiveDocumentStore, SourceBoundLedgerError, SourcePhysicalLineEnding,
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

#[test]
fn actor_joins_candidate_build_to_indexed_giant_physical_line_without_scanning_it() {
    const TEN_MIB: usize = 10 * 1024 * 1024;
    let mut source = String::with_capacity(TEN_MIB + 2);
    source.extend(std::iter::repeat_n('a', TEN_MIB));
    source.push_str("\r\n");
    let (mut document, epoch) = activated_document(&source);

    let joined = document
        .candidate_writer_recognition_line_descriptor(epoch)
        .unwrap();
    assert_eq!(joined.source(), epoch.source());
    assert_eq!(joined.checkpoint().build_id(), epoch.build_id());
    assert_eq!(joined.line_ordinal(), 0);
    assert_eq!(joined.start(), 0);
    assert_eq!(joined.content_end(), TEN_MIB);
    assert_eq!(joined.end(), TEN_MIB + 2);
    assert_eq!(
        joined.physical_line().ending(),
        SourcePhysicalLineEnding::CrLf
    );
    let work = joined.physical_line().receipt();
    assert_eq!(work.tree_nodes_visited, 0);
    assert_eq!(work.boundary_bytes_scanned, 0);
    assert!(work.adjacent_bytes_read <= 2);

    cancel(&mut document, epoch);
}

#[test]
fn descriptor_is_available_only_at_the_untouched_candidate_line_start() {
    let (mut document, epoch) = activated_document("abc\n");
    let initial = document
        .candidate_writer_recognition_line_descriptor(epoch)
        .unwrap();
    assert_eq!(initial.start(), 0);
    assert_eq!(initial.end(), 4);

    let _ = document
        .poll_candidate_writer_recognition(epoch, 1)
        .unwrap();
    assert_eq!(
        document.candidate_writer_recognition_line_descriptor(epoch),
        Err(CandidateWriterError::SourceLedger(
            SourceBoundLedgerError::RecognitionLineNotAtStart
        ))
    );

    cancel(&mut document, epoch);
}

#[test]
fn foreign_epoch_fails_before_the_source_index_is_queried() {
    let (mut document, epoch) = activated_document("local\n");
    let (mut foreign, foreign_epoch) = activated_document("foreign\n");

    assert_eq!(
        document.candidate_writer_recognition_line_descriptor(foreign_epoch),
        Err(CandidateWriterError::Actor(
            LiveDocumentError::WrongCandidateEpoch
        ))
    );

    cancel(&mut document, epoch);
    cancel(&mut foreign, foreign_epoch);
}

#[test]
fn exact_eof_is_a_zero_length_bare_line_descriptor_not_a_phantom_claim() {
    let (mut document, epoch) = activated_document("");
    let joined = document
        .candidate_writer_recognition_line_descriptor(epoch)
        .unwrap();
    assert_eq!(joined.start(), 0);
    assert_eq!(joined.content_end(), 0);
    assert_eq!(joined.end(), 0);
    assert_eq!(
        joined.physical_line().ending(),
        SourcePhysicalLineEnding::BareEof
    );

    cancel(&mut document, epoch);
}
