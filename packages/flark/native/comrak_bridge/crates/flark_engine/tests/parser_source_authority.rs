use flark_engine::parser_internal::{
    M11ParserRangeError, M11ParserRangeStatus, M11ParserSourceRangeAuthority,
};
use flark_engine::{DocumentRuntime, DocumentRuntimeConfig};

fn close_runtime(mut runtime: DocumentRuntime) {
    runtime.begin_close().expect("begin runtime close");
    while !runtime.poll_close(64).expect("runtime close").complete {}
    assert_eq!(runtime.arena_metrics().resident_nodes, 0);
    assert_eq!(runtime.arena_metrics().live_builds, 0);
}

fn read_with_fuel(
    authority: &M11ParserSourceRangeAuthority,
    runtime: &DocumentRuntime,
    fuel: usize,
) -> Vec<u8> {
    let mut cursor = authority.cursor(runtime).expect("range cursor");
    let mut output = [0_u8; 73];
    let mut bytes = Vec::new();
    loop {
        let poll = cursor.poll(fuel, &mut output).expect("bounded range poll");
        assert!(poll.transitions() <= fuel);
        bytes.extend_from_slice(&output[..poll.bytes_read()]);
        if poll.status() == M11ParserRangeStatus::Complete {
            break;
        }
    }
    drop(cursor);
    bytes
}

#[test]
fn one_authority_mints_repeatable_exact_range_cursors_with_independent_fuel() {
    let prefix = "outside:";
    let visible = "αβ `code` and **strong**\n".repeat(400);
    let text = format!("{prefix}{visible}:outside");
    let runtime = DocumentRuntime::new(&text, DocumentRuntimeConfig::default()).expect("runtime");
    let range = prefix.len()..prefix.len() + visible.len();
    let source = runtime.current_source_version().expect("current source");
    let authority = M11ParserSourceRangeAuthority::new(
        &runtime,
        runtime.snapshot_current_source().expect("source lease"),
        range.clone(),
    )
    .expect("source range authority");

    assert_eq!(authority.source(), source);
    assert_eq!(authority.source_range(), range);
    assert_eq!(read_with_fuel(&authority, &runtime, 1), visible.as_bytes());
    assert_eq!(read_with_fuel(&authority, &runtime, 61), visible.as_bytes());

    let mut cancelled = authority.cursor(&runtime).expect("cancelled cursor");
    let mut scratch = [0_u8; 8];
    let poll = cancelled.poll(3, &mut scratch).expect("partial poll");
    assert_eq!(poll.status(), M11ParserRangeStatus::Pending);
    cancelled.cancel();
    drop(cancelled);

    drop(authority);
    close_runtime(runtime);
}

#[test]
fn construction_rejects_noncurrent_and_invalid_source_authority() {
    let text = "é current source";
    let mut runtime =
        DocumentRuntime::new(text, DocumentRuntimeConfig::default()).expect("runtime");
    let stale = runtime.snapshot_current_source().expect("stale lease");
    let current = runtime.current_source_version().expect("current source");
    runtime
        .apply_edit(current, text.len()..text.len(), "!")
        .expect("advance source");
    assert!(matches!(
        M11ParserSourceRangeAuthority::new(&runtime, stale, 0..text.len()),
        Err(M11ParserRangeError::SourceAuthorityMismatch)
    ));

    assert!(matches!(
        M11ParserSourceRangeAuthority::new(
            &runtime,
            runtime.snapshot_current_source().expect("source lease"),
            1..text.len(),
        ),
        Err(M11ParserRangeError::InvalidRange)
    ));
    assert!(matches!(
        M11ParserSourceRangeAuthority::new(
            &runtime,
            runtime.snapshot_current_source().expect("source lease"),
            std::ops::Range { start: 4, end: 3 },
        ),
        Err(M11ParserRangeError::InvalidRange)
    ));
    assert!(matches!(
        M11ParserSourceRangeAuthority::new(
            &runtime,
            runtime.snapshot_current_source().expect("source lease"),
            0..text.len() + 2,
        ),
        Err(M11ParserRangeError::InvalidRange)
    ));

    close_runtime(runtime);
}

#[test]
fn cursor_mint_rechecks_runtime_identity_and_current_source() {
    let text = "runtime-bound range";
    let mut runtime =
        DocumentRuntime::new(text, DocumentRuntimeConfig::default()).expect("runtime");
    let authority = M11ParserSourceRangeAuthority::new(
        &runtime,
        runtime.snapshot_current_source().expect("source lease"),
        0..text.len(),
    )
    .expect("source range authority");

    let foreign =
        DocumentRuntime::new(text, DocumentRuntimeConfig::default()).expect("foreign runtime");
    assert!(matches!(
        authority.cursor(&foreign),
        Err(M11ParserRangeError::WrongRuntime)
    ));
    close_runtime(foreign);

    let current = runtime.current_source_version().expect("current source");
    runtime
        .apply_edit(current, text.len()..text.len(), "!")
        .expect("advance source");
    assert!(matches!(
        authority.cursor(&runtime),
        Err(M11ParserRangeError::SourceAuthorityMismatch)
    ));

    drop(authority);
    close_runtime(runtime);
}
