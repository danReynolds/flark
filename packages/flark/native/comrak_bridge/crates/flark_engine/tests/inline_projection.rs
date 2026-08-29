use flark_engine::parser_internal::{
    M11InlineProjectionBuild, M11InlineProjectionBuildStatus,
    M11InlineProjectionCheckpointQueryPoll, M11InlineProjectionCursorPoll,
    M11InlineProjectionError, M11InlineProjectionFact, M11InlineProjectionKind,
    M11InlineProjectionRoot, M11ParserPageError, M11ParserRangeStatus,
    M11ParserSourceRangeAuthority, M11_INLINE_PROJECTION_FACTS_PER_PAGE_MAX,
};
use flark_engine::{DocumentRuntime, DocumentRuntimeConfig, ParserProfileId};

fn profile(value: u64) -> ParserProfileId {
    ParserProfileId::new(value).expect("nonzero parser profile")
}

fn fact(kind: M11InlineProjectionKind, start: u32) -> M11InlineProjectionFact {
    M11InlineProjectionFact::new(kind, 0, start..start + 5, start + 1..start + 4)
        .expect("valid fact")
}

fn accept_page(
    build: &mut M11InlineProjectionBuild,
    runtime: &mut DocumentRuntime,
    facts: &[M11InlineProjectionFact],
) {
    build.offer_page(facts).expect("offer logical page");
    loop {
        let poll = build.poll(runtime, 1).expect("bounded page poll");
        assert!(poll.transitions() <= 1);
        match poll.status() {
            M11InlineProjectionBuildStatus::NeedsPage => return,
            M11InlineProjectionBuildStatus::Pending => {}
            M11InlineProjectionBuildStatus::Complete
            | M11InlineProjectionBuildStatus::Cancelled => {
                panic!("page build terminated before input closed")
            }
        }
    }
}

fn finish_build(
    build: &mut M11InlineProjectionBuild,
    runtime: &mut DocumentRuntime,
) -> M11InlineProjectionRoot {
    build.finish_input().expect("finish typed input");
    loop {
        let poll = build.poll(runtime, 1).expect("bounded finish poll");
        assert!(poll.transitions() <= 1);
        match poll.status() {
            M11InlineProjectionBuildStatus::Pending => {}
            M11InlineProjectionBuildStatus::Complete => {
                return build.take_root().expect("completed typed root");
            }
            M11InlineProjectionBuildStatus::NeedsPage
            | M11InlineProjectionBuildStatus::Cancelled => {
                panic!("closed typed build requested input or cancelled")
            }
        }
    }
}

fn release_root(root: &mut M11InlineProjectionRoot, runtime: &mut DocumentRuntime) {
    root.begin_release(runtime)
        .expect("begin typed root release");
    loop {
        let poll = root
            .poll_release(runtime, 1)
            .expect("bounded typed root release");
        assert!(poll.receipt().transitions <= 1);
        if poll.complete() {
            break;
        }
    }
}

fn close_runtime(mut runtime: DocumentRuntime) {
    runtime.begin_close().expect("begin runtime close");
    while !runtime.poll_close(64).expect("poll runtime close").complete {}
    assert_eq!(runtime.arena_metrics().resident_nodes, 0);
    assert_eq!(runtime.arena_metrics().live_builds, 0);
}

#[test]
fn source_authority_build_preserves_exact_source_range_and_borrowed_authority() {
    let prefix = "outside:";
    let visible = "*abc*";
    let text = format!("{prefix}{visible}:outside");
    let range = prefix.len()..prefix.len() + visible.len();
    let mut runtime =
        DocumentRuntime::new(&text, DocumentRuntimeConfig::default()).expect("runtime");
    let source = runtime.current_source_version().expect("source");
    let parser_profile = profile(71);
    let authority = M11ParserSourceRangeAuthority::new(
        &runtime,
        runtime.snapshot_current_source().expect("source lease"),
        range.clone(),
    )
    .expect("source authority");
    let mut build =
        M11InlineProjectionBuild::new_from_source_authority(&runtime, &authority, parser_profile)
            .expect("authority-backed typed build");

    let mut authority_cursor = authority.cursor(&runtime).expect("authority retained");
    let mut source_bytes = [0_u8; 8];
    let poll = authority_cursor
        .poll(source_bytes.len(), &mut source_bytes)
        .expect("authority cursor poll");
    assert_eq!(poll.status(), M11ParserRangeStatus::Complete);
    assert_eq!(&source_bytes[..poll.bytes_read()], visible.as_bytes());
    drop(authority_cursor);

    let inline = M11InlineProjectionFact::new(
        M11InlineProjectionKind::Emphasis,
        0,
        0..visible.len() as u32,
        1..visible.len() as u32 - 1,
    )
    .expect("inline fact");
    accept_page(&mut build, &mut runtime, &[inline]);
    let mut root = finish_build(&mut build, &mut runtime);
    assert_eq!(root.descriptor().source(), source);
    assert_eq!(
        root.descriptor().source_range(),
        &(range.start as u32..range.end as u32)
    );
    assert_eq!(root.descriptor().parser_profile(), parser_profile);

    drop(build);
    drop(authority);
    release_root(&mut root, &mut runtime);
    drop(root);
    close_runtime(runtime);
}

#[test]
fn source_authority_build_rejects_wrong_runtime_and_stale_source() {
    let text = "*abc*";
    let mut runtime =
        DocumentRuntime::new(text, DocumentRuntimeConfig::default()).expect("runtime");
    let authority = M11ParserSourceRangeAuthority::new(
        &runtime,
        runtime.snapshot_current_source().expect("source lease"),
        0..text.len(),
    )
    .expect("source authority");

    let foreign =
        DocumentRuntime::new(text, DocumentRuntimeConfig::default()).expect("foreign runtime");
    assert!(matches!(
        M11InlineProjectionBuild::new_from_source_authority(&foreign, &authority, profile(72)),
        Err(M11InlineProjectionError::Pages(
            M11ParserPageError::WrongRuntime
        ))
    ));
    close_runtime(foreign);

    let current = runtime.current_source_version().expect("current source");
    runtime
        .apply_edit(current, text.len()..text.len(), "!")
        .expect("advance source");
    assert!(matches!(
        M11InlineProjectionBuild::new_from_source_authority(&runtime, &authority, profile(72)),
        Err(M11InlineProjectionError::Pages(
            M11ParserPageError::SourceAuthorityMismatch
        ))
    ));

    drop(authority);
    close_runtime(runtime);
}

#[test]
fn typed_root_replays_and_authenticates_exact_source_profile_and_page_order() {
    let text = "abcdefghijklmnopqrstuvwxyz0123456789";
    let mut runtime =
        DocumentRuntime::new(text, DocumentRuntimeConfig::default()).expect("runtime");
    let source = runtime.current_source_version().expect("source");
    let parser_profile = profile(73);
    let lease = runtime.snapshot_current_source().expect("source lease");
    let mut build = M11InlineProjectionBuild::new(&runtime, lease, 0..text.len(), parser_profile)
        .expect("typed build");
    let escape =
        M11InlineProjectionFact::new(M11InlineProjectionKind::BackslashEscape, 0, 0..2, 1..2)
            .expect("escape fact");
    let expected = [
        escape,
        fact(M11InlineProjectionKind::Emphasis, 2),
        fact(M11InlineProjectionKind::Strong, 10),
        fact(M11InlineProjectionKind::Code, 20),
        fact(M11InlineProjectionKind::Strikethrough, 30),
    ];
    accept_page(&mut build, &mut runtime, &expected[..3]);
    accept_page(&mut build, &mut runtime, &expected[3..]);
    let mut root = finish_build(&mut build, &mut runtime);
    assert_eq!(root.descriptor().source(), source);
    assert_eq!(root.descriptor().parser_profile(), parser_profile);
    assert_eq!(root.descriptor().logical_page_count(), 2);
    assert_eq!(root.descriptor().fact_count(), 5);
    assert_ne!(root.descriptor().ordered_commitment256(), [0; 32]);
    drop(build);

    let mut cursor = root
        .cursor(&runtime, source, parser_profile)
        .expect("typed cursor");
    let mut actual = Vec::new();
    loop {
        match cursor.poll(&runtime).expect("validated cursor poll") {
            M11InlineProjectionCursorPoll::Pending { transitions } => {
                assert_eq!(transitions, 1);
            }
            M11InlineProjectionCursorPoll::Fact { transitions, fact } => {
                assert_eq!(transitions, 1);
                actual.push(fact);
            }
            M11InlineProjectionCursorPoll::Complete { transitions } => {
                assert_eq!(transitions, 0);
                break;
            }
        }
    }
    assert_eq!(actual, expected);
    drop(cursor);

    let foreign =
        DocumentRuntime::new(text, DocumentRuntimeConfig::default()).expect("foreign runtime");
    let foreign_source = foreign.current_source_version().expect("foreign source");
    assert!(matches!(
        root.cursor(&runtime, foreign_source, parser_profile),
        Err(M11InlineProjectionError::SourceAuthorityMismatch)
    ));
    assert!(matches!(
        root.cursor(&runtime, source, profile(74)),
        Err(M11InlineProjectionError::ParserProfileMismatch)
    ));
    close_runtime(foreign);

    release_root(&mut root, &mut runtime);
    drop(root);
    close_runtime(runtime);
}

#[test]
fn ordered_commitment_changes_when_same_authority_pages_change_order() {
    let text = "abcdefghijklmnopqrstuvwxyz";
    let mut runtime =
        DocumentRuntime::new(text, DocumentRuntimeConfig::default()).expect("runtime");
    let source = runtime.current_source_version().expect("source");
    let parser_profile = profile(19);
    let emphasis = fact(M11InlineProjectionKind::Emphasis, 4);
    let strong = fact(M11InlineProjectionKind::Strong, 4);

    let mut first_build = M11InlineProjectionBuild::new(
        &runtime,
        runtime.snapshot_current_source().expect("first lease"),
        0..text.len(),
        parser_profile,
    )
    .expect("first build");
    accept_page(&mut first_build, &mut runtime, &[emphasis]);
    accept_page(&mut first_build, &mut runtime, &[strong]);
    let mut first = finish_build(&mut first_build, &mut runtime);
    drop(first_build);

    let mut second_build = M11InlineProjectionBuild::new(
        &runtime,
        runtime.snapshot_current_source().expect("second lease"),
        0..text.len(),
        parser_profile,
    )
    .expect("second build");
    accept_page(&mut second_build, &mut runtime, &[strong]);
    accept_page(&mut second_build, &mut runtime, &[emphasis]);
    let mut second = finish_build(&mut second_build, &mut runtime);
    drop(second_build);

    assert_eq!(first.descriptor().source(), source);
    assert_eq!(second.descriptor().source(), source);
    assert_ne!(
        first.descriptor().ordered_commitment256(),
        second.descriptor().ordered_commitment256()
    );

    release_root(&mut first, &mut runtime);
    release_root(&mut second, &mut runtime);
    drop(first);
    drop(second);
    close_runtime(runtime);
}

#[test]
fn checkpoint_query_is_range_filtered_and_fails_before_exceeding_budget() {
    let text = "abcdefghijklmnopqrstuvwxyz0123456789";
    let mut runtime =
        DocumentRuntime::new(text, DocumentRuntimeConfig::default()).expect("runtime");
    let source = runtime.current_source_version().expect("source");
    let parser_profile = profile(23);
    let mut build = M11InlineProjectionBuild::new(
        &runtime,
        runtime.snapshot_current_source().expect("lease"),
        0..text.len(),
        parser_profile,
    )
    .expect("build");
    let facts = [
        fact(M11InlineProjectionKind::Emphasis, 2),
        fact(M11InlineProjectionKind::Strong, 12),
        fact(M11InlineProjectionKind::Code, 24),
    ];
    accept_page(&mut build, &mut runtime, &facts);
    let mut root = finish_build(&mut build, &mut runtime);
    drop(build);

    let mut query = root
        .begin_checkpoint_query(&runtime, source, parser_profile, 13..15, 16)
        .expect("query");
    let mut matches = Vec::new();
    loop {
        match query.poll(&runtime).expect("bounded query poll") {
            M11InlineProjectionCheckpointQueryPoll::Pending { .. } => {}
            M11InlineProjectionCheckpointQueryPoll::Fact { fact, .. } => matches.push(fact),
            M11InlineProjectionCheckpointQueryPoll::Complete { .. } => break,
        }
    }
    assert_eq!(matches, [facts[1]]);
    drop(query);

    let mut exhausted = root
        .begin_checkpoint_query(&runtime, source, parser_profile, 13..15, 1)
        .expect("budgeted query");
    let _ = exhausted.poll(&runtime).expect("first admitted query poll");
    assert!(matches!(
        exhausted.poll(&runtime),
        Err(M11InlineProjectionError::QueryBudgetExceeded)
    ));
    drop(exhausted);

    release_root(&mut root, &mut runtime);
    drop(root);
    close_runtime(runtime);
}

#[test]
fn cancellation_and_release_return_all_arena_owners() {
    let text = "abcdefghijklmnopqrstuvwxyz";
    let mut runtime =
        DocumentRuntime::new(text, DocumentRuntimeConfig::default()).expect("runtime");
    let mut build = M11InlineProjectionBuild::new(
        &runtime,
        runtime.snapshot_current_source().expect("lease"),
        0..text.len(),
        profile(29),
    )
    .expect("build");
    build
        .offer_page(&[fact(M11InlineProjectionKind::Strong, 3)])
        .expect("offer page");
    let _ = build.poll(&mut runtime, 1).expect("start page work");
    build.begin_cancel(&mut runtime).expect("begin cancel");
    loop {
        let poll = build.poll_cancel(&mut runtime, 1).expect("bounded cancel");
        assert!(poll.receipt().transitions <= 1);
        if poll.complete() {
            break;
        }
    }
    drop(build);
    assert_eq!(runtime.arena_metrics().resident_nodes, 0);
    assert_eq!(runtime.arena_metrics().live_builds, 0);
    close_runtime(runtime);
}

#[test]
fn persistent_projection_exceeds_128_physical_pages_without_flat_role_limit() {
    const LOGICAL_PAGES: usize = 2_100;

    let text = "*x*";
    let mut runtime =
        DocumentRuntime::new(text, DocumentRuntimeConfig::default()).expect("runtime");
    let source = runtime.current_source_version().expect("source");
    let parser_profile = profile(31);
    let repeated = M11InlineProjectionFact::new(M11InlineProjectionKind::Emphasis, 0, 0..3, 1..2)
        .expect("fact");
    let page = [repeated; M11_INLINE_PROJECTION_FACTS_PER_PAGE_MAX];
    let mut build = M11InlineProjectionBuild::new(
        &runtime,
        runtime.snapshot_current_source().expect("lease"),
        0..text.len(),
        parser_profile,
    )
    .expect("build");
    for _ in 0..LOGICAL_PAGES {
        accept_page(&mut build, &mut runtime, &page);
    }
    let mut root = finish_build(&mut build, &mut runtime);
    assert_eq!(root.descriptor().logical_page_count(), LOGICAL_PAGES as u64);
    assert_eq!(
        root.descriptor().fact_count(),
        (LOGICAL_PAGES * M11_INLINE_PROJECTION_FACTS_PER_PAGE_MAX) as u64
    );
    assert!(root.descriptor().storage_page_count() > 128);
    drop(build);

    let mut cursor = root
        .cursor(&runtime, source, parser_profile)
        .expect("cursor");
    let mut facts = 0_u64;
    loop {
        match cursor.poll(&runtime).expect("authenticated replay") {
            M11InlineProjectionCursorPoll::Pending { .. } => {}
            M11InlineProjectionCursorPoll::Fact { .. } => facts += 1,
            M11InlineProjectionCursorPoll::Complete { .. } => break,
        }
    }
    assert_eq!(facts, root.descriptor().fact_count());
    drop(cursor);

    release_root(&mut root, &mut runtime);
    drop(root);
    close_runtime(runtime);
}
