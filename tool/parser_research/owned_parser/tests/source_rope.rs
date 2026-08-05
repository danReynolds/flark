use flark_owned_parser_trial::SourceRope;

#[test]
fn persistent_rope_preserves_utf8_and_shares_local_edits() {
    let mut source = String::new();
    for index in 0..10_000 {
        source.push_str(&format!("line {index} 😀\n"));
    }
    let rope = SourceRope::from_str(&source);
    assert_eq!(rope.materialize(), source);
    assert_eq!(rope.newline_count(), 10_000);
    assert!(rope.leaf_count() > 10);

    let needle = "line 5000";
    let start = source.find(needle).unwrap();
    let edited = rope.replace(start, start + needle.len(), "changed 50");
    let expected = source.replacen(needle, "changed 50", 1);
    assert_eq!(edited.materialize(), expected);
    assert_eq!(
        rope.materialize(),
        source,
        "prior revision must remain exact"
    );
    assert!(edited.height() < 32);
    assert!(edited.leaf_count() <= rope.leaf_count() + 3);
}

#[test]
fn line_reads_cross_leaf_boundaries() {
    let source = format!("{}\nend", "x".repeat(SourceRope::CHUNK_SIZE + 37));
    let rope = SourceRope::from_str(&source);
    let (end, line) = rope.line_from(0);
    assert_eq!(end, source.find('\n').unwrap() + 1);
    assert_eq!(line, &source[..end]);
    assert_eq!(rope.line_from(end).1, "end");
}
