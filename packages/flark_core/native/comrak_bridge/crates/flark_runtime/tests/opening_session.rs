//! Headless dress rehearsal for the RFC 029 A3 receipt: a progressive open
//! through the real `DocumentSession` API — paged admission, a certified
//! viewport queryable before EOF, a literal edit during load, sealing, and
//! final equality with the complete-source oracle.

use flark_runtime::{
    DocumentLiveViewportSpan, DocumentSession, DocumentSessionPhase, DocumentViewport,
};

const PAGE_BYTES: usize = 8 * 1024;

fn fixture() -> String {
    let mut source = String::new();
    for index in 0..2_000 {
        source.push_str(&format!(
            "Paragraph {index} has a [link](https://example.invalid/{index}) and **bold** text.\n\n"
        ));
    }
    source
}

fn pump_until_idle(session: &mut DocumentSession) {
    // A bounded number of grants; the opening pump yields when it needs
    // transport, so this cannot spin.
    for _ in 0..64 {
        session.pump(512).expect("pump opening session");
    }
}

fn certified_span(session: &mut DocumentSession) -> Option<std::ops::Range<u64>> {
    let revision = session.revision();
    let viewport = session
        .query_live_viewport(revision, 0..session.source_byte_len(), 16)
        .expect("live viewport");
    viewport.spans.iter().find_map(|span| match span {
        DocumentLiveViewportSpan::CertifiedUnchanged { source_range, .. } => {
            Some(source_range.clone())
        }
        _ => None,
    })
}

fn certified_rows(session: &mut DocumentSession, end: usize) -> DocumentViewport {
    let revision = session.revision();
    session
        .query_viewport(revision, 0..end, 64)
        .expect("certified opening rows")
}

/// Release-mode timing probe for the session-layer open path. Run:
/// `cargo test --release -p flark-runtime --features opening-session \
///    --test opening_session -- --ignored --nocapture`
#[test]
#[ignore = "release-mode session-layer timing probe"]
fn session_layer_first_certified_viewport_timing() {
    const BLOCK: &str = "Ordinary paragraph with **bold** text and plain words.\n\n";
    let target = 10 * 1024 * 1024;
    let mut source = String::with_capacity(target + BLOCK.len());
    while source.len() < target {
        source.push_str(BLOCK);
    }

    let started = std::time::Instant::now();
    let mut session = DocumentSession::begin_opening().expect("begin opening");
    let mut offset = 0;
    let mut admitted_at_certification = 0;
    let mut first_certified = None;
    while offset < source.len() {
        let end = source.len().min(offset + PAGE_BYTES);
        session
            .opening_append_page(&source[offset..end])
            .expect("append page");
        offset = end;
        session.pump(512).expect("pump");
        if first_certified.is_none() {
            if let Some(span) = certified_span(&mut session) {
                first_certified = Some((started.elapsed(), span.end));
                admitted_at_certification = offset;
            }
        }
    }
    session.seal_opening().expect("seal");
    while session.phase() != DocumentSessionPhase::Ready {
        session.pump(4_096).expect("pump to ready");
    }
    let ready = started.elapsed();
    let (first, span_end) = first_certified.expect("certification before EOF");
    let rows = certified_rows(&mut session, span_end as usize);
    println!(
        "{{\"probe\":\"session_layer_open\",\"source_bytes\":{},\"first_certified_ms\":{:.3},\"admitted_bytes_at_certification\":{},\"certified_span_bytes\":{},\"rows\":{},\"ready_ms\":{:.3}}}",
        source.len(),
        first.as_secs_f64() * 1000.0,
        admitted_at_certification,
        span_end,
        rows.rows.len(),
        ready.as_secs_f64() * 1000.0,
    );
    session.close().expect("close");
}

#[test]
fn progressive_open_serves_certified_rows_before_eof_and_matches_the_oracle() {
    let source = fixture();
    let mut session = DocumentSession::begin_opening().expect("begin opening");
    assert_eq!(session.phase(), DocumentSessionPhase::Building);

    // Admit the first two pages and pump: the certified viewport must appear
    // long before EOF, with the tail still pending.
    let mut offset = 0;
    let mut admit_page = |session: &mut DocumentSession, offset: &mut usize| {
        let end = source.len().min(*offset + PAGE_BYTES);
        session
            .opening_append_page(&source[*offset..end])
            .expect("append page");
        *offset = end;
    };
    admit_page(&mut session, &mut offset);
    admit_page(&mut session, &mut offset);
    pump_until_idle(&mut session);

    let span = certified_span(&mut session).expect("pre-EOF certified span");
    assert_eq!(span.start, 0);
    assert!(span.end > 0);
    assert!(
        (span.end as usize) < source.len(),
        "certification precedes EOF"
    );
    let early = certified_rows(&mut session, span.end as usize);
    assert_eq!(early.start_ordinal, 0);
    assert!(!early.rows.is_empty());
    assert!(!early.complete, "a pre-EOF row count is not an exact total");

    // A literal edit during load: the store is the authority, everything
    // rebuilds, and certification returns at the new revision.
    let revision = session.revision();
    let edit_at = source.find("bold").expect("first bold");
    session
        .apply_edit(revision, edit_at..edit_at + "bold".len(), "BOLD")
        .expect("edit during load");
    assert!(session.revision() > revision);
    assert!(
        session
            .query_viewport(revision, 0..span.end as usize, 64)
            .is_err(),
        "the pre-edit revision is stale"
    );
    pump_until_idle(&mut session);
    let re_span = certified_span(&mut session).expect("recertified after load edit");
    let re_rows = certified_rows(&mut session, re_span.end as usize);
    assert!(!re_rows.rows.is_empty());
    assert_eq!(
        session
            .source_bytes(edit_at..edit_at + "BOLD".len())
            .expect("edited source bytes"),
        b"BOLD"
    );

    // Stream the rest, seal, and pump to full readiness.
    while offset < source.len() {
        admit_page(&mut session, &mut offset);
        pump_until_idle(&mut session);
    }
    session.seal_opening().expect("seal");
    while session.phase() != DocumentSessionPhase::Ready {
        session.pump(4_096).expect("pump to ready");
    }

    // Oracle: the complete edited source parsed the ordinary way. The final
    // viewport rows over the early range must be identical, including the
    // load-time edit.
    let mut edited = source.clone();
    edited.replace_range(edit_at..edit_at + "bold".len(), "BOLD");
    let mut oracle = DocumentSession::begin(&edited).expect("begin oracle");
    while oracle.phase() != DocumentSessionPhase::Ready {
        oracle.pump(4_096).expect("pump oracle");
    }
    let end = re_span.end as usize;
    let final_rows = session
        .query_viewport(session.revision(), 0..end, 64)
        .expect("final rows");
    let oracle_rows = oracle
        .query_viewport(oracle.revision(), 0..end, 64)
        .expect("oracle rows");
    assert_eq!(final_rows.rows, oracle_rows.rows);
    assert_eq!(session.source_byte_len(), edited.len());

    session.close().expect("close opening session");
    oracle.close().expect("close oracle");
}
