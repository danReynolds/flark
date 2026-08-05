use flark_v3_runtime_slice::{
    CoordinatorError, LiveDocumentError, LiveDocumentStore, ParseGeneration, ParseToken,
    SourceError, SourceRevision, SourceRootId, SourceSnapshotDescriptor, SourceStoreError,
};

fn initial_token(document: &LiveDocumentStore) -> ParseToken {
    document
        .active_parse_plan()
        .expect("initial parse is admitted")
        .token
}

#[test]
fn empty_and_nonempty_revision_zero_documents_admit_generation_one() {
    for source in ["", "alpha 😀\r\n"] {
        let mut document = LiveDocumentStore::new(source, 8).unwrap();
        let token = initial_token(&document);
        assert_eq!(token.generation, ParseGeneration(1));
        assert_eq!(token.source_revision, SourceRevision(0));
        assert_ne!(token.source_root, SourceRootId(0));
        assert_eq!(token.source_root, document.source_root());

        let epoch = document.begin_candidate(token).unwrap();
        assert_eq!(epoch.parse_token(), token);
        assert_eq!(epoch.source().revision, SourceRevision(0));
        assert_eq!(epoch.source().root, token.source_root);
        assert_eq!(epoch.source().bytes, source.len());
        assert_eq!(
            epoch.arena_identity(),
            document.current_output().arena_root.arena()
        );

        let mut actual = Vec::new();
        while let Some(byte) = document.poll_candidate_byte(epoch).unwrap() {
            assert_eq!(byte.root, token.source_root);
            assert_eq!(byte.offset, actual.len());
            actual.push(byte.byte);
        }
        assert_eq!(actual, source.as_bytes());
        assert_eq!(
            document.candidate_cursor_offset(epoch).unwrap(),
            source.len()
        );
    }
}

#[test]
fn invalid_live_document_configuration_is_reported_without_panicking() {
    assert!(matches!(
        LiveDocumentStore::new("abc", 0),
        Err(LiveDocumentError::Source(
            SourceStoreError::InvalidLineageCapacity
        ))
    ));
}

#[test]
fn candidate_issuance_is_linear_and_rejects_stale_or_wrong_tokens() {
    let mut document = LiveDocumentStore::new("abc", 8).unwrap();
    let token = initial_token(&document);

    let stale = ParseToken {
        generation: ParseGeneration(0),
        ..token
    };
    assert_eq!(
        document.begin_candidate(stale),
        Err(LiveDocumentError::Coordinator(
            CoordinatorError::StaleGeneration {
                supplied: ParseGeneration(0),
                current: ParseGeneration(1),
            }
        ))
    );
    let wrong = ParseToken {
        source_root: SourceRootId(token.source_root.0 + 1),
        ..token
    };
    assert_eq!(
        document.begin_candidate(wrong),
        Err(LiveDocumentError::Coordinator(
            CoordinatorError::WrongParseToken
        ))
    );

    let epoch = document.begin_candidate(token).unwrap();
    assert_eq!(
        document.begin_candidate(token),
        Err(LiveDocumentError::CandidateAlreadyActive)
    );
    assert_eq!(document.candidate_epoch(), Some(epoch));
    assert_eq!(
        document.poll_candidate_byte(epoch).unwrap().unwrap().byte,
        b'a'
    );
}

#[test]
fn cancellation_invalidates_epoch_and_recycled_build_generation() {
    let mut document = LiveDocumentStore::new("abc", 8).unwrap();
    let token = initial_token(&document);
    let first = document.begin_candidate(token).unwrap();
    let abort = document.cancel_candidate(first).unwrap();

    assert_eq!(
        document.poll_candidate_byte(first),
        Err(LiveDocumentError::NoCandidate)
    );
    let receipt = document.poll_candidate_abort(abort, 0).unwrap();
    assert!(receipt.complete);
    assert_eq!(receipt.owners_scheduled, 0);
    assert_eq!(
        document.poll_candidate_abort(abort, 1),
        Err(LiveDocumentError::UnknownAbort)
    );

    let second = document.begin_candidate(token).unwrap();
    assert_ne!(first.build_id(), second.build_id());
    assert_eq!(
        document.poll_candidate_byte(first),
        Err(LiveDocumentError::WrongCandidateEpoch)
    );
    assert_eq!(
        document.poll_candidate_byte(second).unwrap().unwrap().byte,
        b'a'
    );
}

#[test]
fn an_epoch_from_another_live_document_cannot_alias_local_candidate_work() {
    let mut left = LiveDocumentStore::new("same", 8).unwrap();
    let mut right = LiveDocumentStore::new("same", 8).unwrap();
    let left_epoch = left.begin_candidate(initial_token(&left)).unwrap();
    let right_epoch = right.begin_candidate(initial_token(&right)).unwrap();

    assert_ne!(left_epoch.arena_identity(), right_epoch.arena_identity());
    assert_eq!(
        left.poll_candidate_byte(right_epoch),
        Err(LiveDocumentError::WrongCandidateEpoch)
    );
    assert_eq!(
        left.poll_candidate_byte(left_epoch).unwrap().unwrap().byte,
        b's'
    );
}

#[test]
fn cancellation_burns_every_minted_entity_identity() {
    let mut document = LiveDocumentStore::new("abc", 8).unwrap();
    let token = initial_token(&document);
    let first = document.begin_candidate(token).unwrap();
    let first_block = document.mint_block_permit(first).unwrap();
    let first_coverage = document.mint_coverage_permit(first).unwrap();
    assert_eq!(first_block.build_id(), first.build_id());
    assert_eq!(first_coverage.build_id(), first.build_id());

    let abort = document.cancel_candidate(first).unwrap();
    assert!(document.poll_candidate_abort(abort, 1).unwrap().complete);
    drop(first_block);
    drop(first_coverage);

    let second = document.begin_candidate(token).unwrap();
    let second_block = document.mint_block_permit(second).unwrap();
    let second_coverage = document.mint_coverage_permit(second).unwrap();
    assert_eq!(second_block.id().0, 2);
    assert_eq!(second_coverage.id().0, 2);
    assert_eq!(second_block.build_id(), second.build_id());
    assert_eq!(second_coverage.build_id(), second.build_id());
}

#[test]
fn one_edit_publishes_both_clocks_and_detaches_the_old_candidate() {
    let mut document = LiveDocumentStore::new("a😀b", 8).unwrap();
    let old_token = initial_token(&document);
    let old_epoch = document.begin_candidate(old_token).unwrap();
    let expected = document.source_descriptor();
    let before = document.clocks();
    assert!(before.source_and_coordinator_are_aligned());

    let receipt = document.accept_edit(expected, 1..5, "X").unwrap();
    let transition = receipt.source().transition;
    assert_eq!(transition.base_revision, SourceRevision(0));
    assert_eq!(transition.base_root, expected.root);
    assert_eq!(transition.target_revision, SourceRevision(1));
    assert_ne!(transition.result_root, expected.root);
    assert_eq!(document.query_source().materialize_for_testing(), "aXb");

    let after = document.clocks();
    assert!(after.source_and_coordinator_are_aligned());
    assert_eq!(after.source().revision, SourceRevision(1));
    assert_eq!(after.source().root, transition.result_root);
    assert_eq!(after.source().bytes, 3);
    assert_eq!(after.parse_generation(), ParseGeneration(2));
    assert_eq!(after.active(), Some(old_token));
    let queued = after.queued().expect("newest revision is queued");
    assert_eq!(queued.generation, ParseGeneration(2));
    assert_eq!(queued.source_revision, SourceRevision(1));
    assert_eq!(queued.source_root, transition.result_root);
    assert_eq!(receipt.admission().queued.unwrap().token, queued);
    assert_eq!(document.candidate_epoch(), None);
    assert_eq!(
        document.poll_candidate_byte(old_epoch),
        Err(LiveDocumentError::NoCandidate)
    );

    let abort = receipt.cancelled().expect("old parser job was detached");
    assert!(document.poll_candidate_abort(abort, 0).unwrap().complete);
    let promotion = document.promote_latest_parse().unwrap();
    assert_eq!(promotion.cancelled, old_token);
    assert_eq!(promotion.promoted.token, queued);
    assert_eq!(initial_token(&document), queued);
    let next_epoch = document.begin_candidate(queued).unwrap();
    assert_eq!(next_epoch.source(), after.source());
}

#[test]
fn every_edit_preflight_failure_preserves_both_clocks_and_candidate() {
    let mut document = LiveDocumentStore::new("a😀z", 8).unwrap();
    let token = initial_token(&document);
    let epoch = document.begin_candidate(token).unwrap();
    assert_eq!(
        document.poll_candidate_byte(epoch).unwrap().unwrap().byte,
        b'a'
    );
    let expected = document.source_descriptor();
    let before_clocks = document.clocks();
    let before_source = document.query_source().materialize_for_testing();
    let before_offset = document.candidate_cursor_offset(epoch).unwrap();

    let stale_revision = SourceSnapshotDescriptor {
        revision: SourceRevision(1),
        ..expected
    };
    let wrong_root = SourceSnapshotDescriptor {
        root: SourceRootId(expected.root.0 + 1),
        ..expected
    };
    let wrong_length = SourceSnapshotDescriptor {
        bytes: expected.bytes + 1,
        ..expected
    };
    for supplied in [stale_revision, wrong_root, wrong_length] {
        assert_eq!(
            document.accept_edit(supplied, 0..1, "q"),
            Err(LiveDocumentError::Source(
                SourceStoreError::SnapshotMismatch {
                    expected: supplied,
                    actual: expected,
                }
            ))
        );
        assert_eq!(document.clocks(), before_clocks);
        assert_eq!(
            document.query_source().materialize_for_testing(),
            before_source
        );
        assert_eq!(document.candidate_epoch(), Some(epoch));
        assert_eq!(
            document.candidate_cursor_offset(epoch).unwrap(),
            before_offset
        );
    }

    assert_eq!(
        document.accept_edit(expected, 99..99, "q"),
        Err(LiveDocumentError::Source(SourceStoreError::Source(
            SourceError::InvalidRange
        )))
    );
    assert_eq!(document.clocks(), before_clocks);
    assert_eq!(document.candidate_epoch(), Some(epoch));

    assert_eq!(
        document.accept_edit(expected, 2..3, "q"),
        Err(LiveDocumentError::Source(SourceStoreError::Source(
            SourceError::NotCharBoundary(2)
        )))
    );
    assert_eq!(document.clocks(), before_clocks);
    assert_eq!(
        document.query_source().materialize_for_testing(),
        before_source
    );
    assert_eq!(document.candidate_epoch(), Some(epoch));
    assert_eq!(
        document.candidate_cursor_offset(epoch).unwrap(),
        before_offset
    );
}

#[test]
fn repeated_edits_replace_only_the_queued_plan_and_never_split_clocks() {
    let mut document = LiveDocumentStore::new("a", 8).unwrap();
    let original = initial_token(&document);
    let initial_descriptor = document.source_descriptor();

    let first = document.accept_edit(initial_descriptor, 1..1, "b").unwrap();
    let first_queued = first.admission().queued.unwrap().token;
    assert_eq!(first_queued.generation, ParseGeneration(2));
    assert!(document.clocks().source_and_coordinator_are_aligned());

    let second = document
        .accept_edit(document.source_descriptor(), 2..2, "c")
        .unwrap();
    let second_admission = second.admission();
    assert_eq!(second_admission.active.token, original);
    assert_eq!(second_admission.replaced_queued, Some(first_queued));
    let newest = second_admission.queued.unwrap().token;
    assert_eq!(newest.generation, ParseGeneration(3));
    assert_eq!(newest.source_revision, SourceRevision(2));
    assert_eq!(document.query_source().materialize_for_testing(), "abc");
    assert!(document.clocks().source_and_coordinator_are_aligned());
    assert_eq!(document.clocks().queued(), Some(newest));

    let before_failure = document.clocks();
    assert!(matches!(
        document.accept_edit(initial_descriptor, 0..0, "x"),
        Err(LiveDocumentError::Source(
            SourceStoreError::SnapshotMismatch { .. }
        ))
    ));
    assert_eq!(document.clocks(), before_failure);

    let promotion = document.promote_latest_parse().unwrap();
    assert_eq!(promotion.promoted.token, newest);
    assert_eq!(document.clocks().active(), Some(newest));
    assert_eq!(document.clocks().queued(), None);
}
