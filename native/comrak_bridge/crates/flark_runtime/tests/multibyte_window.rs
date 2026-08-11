use flark_runtime::{DocumentActor, DocumentSessionPhase};

/// One long line of family emoji: 11 UTF-16 units and 25 UTF-8 bytes per
/// cluster, so a byte budget almost never lands on a scalar boundary.
fn emoji_line(target_bytes: usize) -> String {
    const FAMILY: &str = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}\u{200D}\u{1F466}";
    let mut source = String::with_capacity(target_bytes + 32);
    while source.len() < target_bytes {
        source.push_str(FAMILY);
    }
    source.push('\n');
    source
}

#[test]
fn a_byte_capped_viewport_request_snaps_instead_of_failing() {
    let source = emoji_line(20 * 1024);
    // 16 KiB is the host's visible-byte budget, and it lands inside a
    // cluster: exactly the case ASCII fixtures never exercise.
    let cap = 16 * 1024;
    assert!(
        !source.is_char_boundary(cap),
        "fixture must exercise a mid-scalar cap"
    );

    let actor = DocumentActor::begin(source.clone()).expect("actor");
    while actor.pump(4096).expect("pump").phase != DocumentSessionPhase::Ready {}
    let revision = actor.inspect().expect("inspect").revision;

    let viewport = actor
        .query_viewport(revision, 0..cap, 32)
        .expect("a byte-capped request must be served, not rejected");
    assert!(viewport.requested_range.end <= cap as u64);
    assert!(
        source.is_char_boundary(viewport.requested_range.end as usize),
        "the served range must end on a scalar boundary"
    );

    // The same cap through the live projection path.
    let live = actor
        .query_live_viewport(revision, 0..cap, 32)
        .expect("live viewport must also be served");
    assert!(live.covered_range.end <= cap as u64);
    assert!(source.is_char_boundary(live.covered_range.end as usize));
}
