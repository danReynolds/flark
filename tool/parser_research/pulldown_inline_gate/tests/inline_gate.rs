use flark_pulldown_inline_gate::{
    parse_to_completion, CancellationToken, Fact, FactKind, InlineEngine, LogicalLeaf, ParsePoll,
    ReferenceTable, Segment,
};
use pulldown_cmark::{Event, Parser, Tag};
use std::ops::Range;
use std::sync::Arc;

fn parse(source: &str, fuel: usize, references: Arc<ReferenceTable>) -> InlineEngine {
    parse_to_completion(LogicalLeaf::contiguous(source), references, fuel)
}

fn style_ranges(facts: &[Fact]) -> Vec<(&'static str, Range<usize>)> {
    let mut ranges: Vec<_> = facts
        .iter()
        .filter_map(|fact| match fact.kind {
            FactKind::Emphasis { .. } => Some(("em", fact.range.clone())),
            FactKind::Strong { .. } => Some(("strong", fact.range.clone())),
            FactKind::CodeSpan { .. } => Some(("code", fact.range.clone())),
            FactKind::InlineLink { .. } => Some(("link", fact.range.clone())),
            FactKind::ReferenceLink { .. } => Some(("ref", fact.range.clone())),
            FactKind::UnresolvedReference { .. } => None,
        })
        .collect();
    ranges.sort_by_key(|(kind, range)| (range.start, range.end, *kind));
    ranges
}

fn pulldown_ranges(source: &str) -> Vec<(&'static str, Range<usize>)> {
    let mut ranges: Vec<_> = Parser::new(source)
        .into_offset_iter()
        .filter_map(|(event, range)| match event {
            Event::Start(Tag::Emphasis) => Some(("em", range)),
            Event::Start(Tag::Strong) => Some(("strong", range)),
            Event::Start(Tag::Link { .. }) => Some(("link", range)),
            Event::Code(_) => Some(("code", range)),
            _ => None,
        })
        .collect();
    ranges.sort_by_key(|(kind, range)| (range.start, range.end, *kind));
    ranges
}

#[test]
fn selected_clean_semantics_match_pulldown_0134() {
    let cases = [
        "*em*",
        "**strong**",
        "***both***",
        "a_b_c and a*b*c",
        "*outer **inner** outer*",
        "`code` and `` co`de ``",
        "`soft\nline`",
        "[label](destination)",
        "[*label*](<destination> \"title\")",
        "[nested [brackets]](url)",
        "escaped \\*literal* then _live_",
        "Unicode **élan** and _東京_",
    ];
    for source in cases {
        let ours = parse(source, 2, Arc::new(ReferenceTable::new()));
        assert_eq!(
            style_ranges(ours.facts()),
            pulldown_ranges(source),
            "source={source:?}"
        );
    }
}

#[test]
fn inline_link_range_matrix_matches_pulldown_0134() {
    let cases = [
        "[link](/uri \"title\")",
        "[link](/uri 'title')",
        "[link](/uri (title))",
        "[link](<>)",
        "[link](foo(and(bar)))",
        "[link](foo\\(bar\\))",
        "[link](foo \"ti\\\"tle\")",
        "[link](foo\n\"title\")",
        "[link](foo\n\n\"title\")",
        "[link](<foo bar>)",
        "[link](foo(and(bar))))",
        "[a [b] c](url)",
        "[[inner](one)](two)",
        "[*em* and `code`](url)",
        "before [link](url) after",
    ];
    for source in cases {
        let ours = parse(source, 1, Arc::new(ReferenceTable::new()));
        let ours: Vec<_> = style_ranges(ours.facts())
            .into_iter()
            .filter(|(kind, _)| *kind == "link")
            .collect();
        let pulldown: Vec<_> = pulldown_ranges(source)
            .into_iter()
            .filter(|(kind, _)| *kind == "link")
            .collect();
        assert_eq!(ours, pulldown, "source={source:?}");
    }
}

#[test]
fn randomized_emphasis_semantics_match_pulldown_0134() {
    let alphabet = b"*_ab .";
    let mut state = 0x5eed_cafe_u64;
    for case in 0..5_000 {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        let len = usize::try_from(state % 48).unwrap() + 1;
        let mut source = String::with_capacity(len);
        for _ in 0..len {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1);
            let index = usize::try_from(state % alphabet.len() as u64).unwrap();
            source.push(alphabet[index] as char);
        }
        let ours = parse(&source, 3, Arc::new(ReferenceTable::new()));
        let ours: Vec<_> = style_ranges(ours.facts())
            .into_iter()
            .filter(|(kind, _)| matches!(*kind, "em" | "strong"))
            .collect();
        let pulldown: Vec<_> = pulldown_ranges(&source)
            .into_iter()
            .filter(|(kind, _)| matches!(*kind, "em" | "strong"))
            .collect();
        assert_eq!(ours, pulldown, "case={case}, source={source:?}");
    }
}

#[test]
fn randomized_code_span_ranges_match_pulldown_0134() {
    // One logical leaf: blank lines are a block-boundary concern and must be
    // segmented by the block machine before this inline engine is invoked.
    let alphabet = b"`ab ";
    let mut state = 0xc0de_5eed_u64;
    for case in 0..5_000 {
        state = state
            .wrapping_mul(2_862_933_555_777_941_757)
            .wrapping_add(3_037_000_493);
        let len = usize::try_from(state % 64).unwrap() + 1;
        let mut source = String::with_capacity(len);
        for _ in 0..len {
            state = state
                .wrapping_mul(2_862_933_555_777_941_757)
                .wrapping_add(3_037_000_493);
            let index = usize::try_from(state % alphabet.len() as u64).unwrap();
            source.push(alphabet[index] as char);
        }
        let ours = parse(&source, 1, Arc::new(ReferenceTable::new()));
        let ours: Vec<_> = style_ranges(ours.facts())
            .into_iter()
            .filter(|(kind, _)| *kind == "code")
            .collect();
        let pulldown: Vec<_> = pulldown_ranges(&source)
            .into_iter()
            .filter(|(kind, _)| *kind == "code")
            .collect();
        assert_eq!(ours, pulldown, "case={case}, source={source:?}");
    }
}

#[test]
fn exact_direct_ranges_cover_the_selected_profile() {
    let mut references = ReferenceTable::new();
    references.define("ID", 42);
    let source = "*a **b** c* and ` x ` plus [link](dest \"title\") and [ref][id]";
    let engine = parse(source, 3, Arc::new(references));
    let facts = engine.canonical_facts();

    assert!(facts.iter().any(|fact| {
        fact.range == (0..11)
            && matches!(
                fact.kind,
                FactKind::Emphasis {
                    ref opener,
                    ref closer
                } if opener == &(0..1) && closer == &(10..11)
            )
    }));
    assert!(facts.iter().any(|fact| {
        fact.range == (3..8)
            && matches!(
                fact.kind,
                FactKind::Strong {
                    ref opener,
                    ref closer
                } if opener == &(3..5) && closer == &(6..8)
            )
    }));
    assert!(facts.iter().any(|fact| {
        fact.range == (16..21)
            && matches!(
                fact.kind,
                FactKind::CodeSpan {
                    ref opener,
                    ref content,
                    ref closer,
                    trim_one_space: true,
                } if opener == &(16..17) && content == &(17..20) && closer == &(20..21)
            )
    }));
    assert!(facts.iter().any(|fact| {
        fact.range == (27..47)
            && matches!(
                fact.kind,
                FactKind::InlineLink {
                    ref label,
                    ref destination,
                    title: Some(ref title),
                } if label == &(28..32) && destination == &(34..38) && title == &(40..45)
            )
    }));
    assert!(facts.iter().any(|fact| {
        fact.range == (52..61)
            && matches!(
                fact.kind,
                FactKind::ReferenceLink {
                    ref label,
                    ref reference,
                    dependency_id: 42,
                    ..
                } if label == &(53..56) && reference == &(58..60)
            )
    }));
}

#[test]
fn resumption_is_identical_after_every_revision() {
    let revisions = [
        "plain",
        "*plain*",
        "**plain**",
        "**plain** and `code`",
        "**plain** and `` co`de ``",
        "[**plain**](https://example.test/a_(b))",
        "before [name][target] after",
        "before [name][TARGET] after *now*",
        "before [name][missing] after *now*",
        "escaped \\*literal* and _live_",
        "unicode *élan* and [Σ][σ]",
    ];
    let mut references = ReferenceTable::new();
    references.define("target", 7);
    references.define("Σ", 8);
    let references = Arc::new(references);

    for revision in revisions {
        let clean = parse(revision, usize::MAX / 4, references.clone());
        for fuel in [1, 2, 7, 31] {
            let resumed = parse(revision, fuel, references.clone());
            assert_eq!(
                resumed.canonical_facts(),
                clean.canonical_facts(),
                "revision={revision:?}, fuel={fuel}"
            );
            assert!(resumed.memory_receipt().max_poll_work <= fuel);
        }
    }
}

#[test]
fn segmented_leaf_preserves_virtual_bytes_without_fake_source_ranges() {
    let source = "left> **right**";
    let leaf = LogicalLeaf::segmented(
        source,
        vec![
            Segment::Source(0..4),
            Segment::VirtualNewline { anchor: 4 },
            Segment::Source(6..15),
        ],
    )
    .unwrap();
    let engine = parse_to_completion(leaf, Arc::new(ReferenceTable::new()), 2);
    let strong = engine
        .facts()
        .iter()
        .find(|fact| matches!(fact.kind, FactKind::Strong { .. }))
        .unwrap();
    assert_eq!(strong.range, 5..14);
    assert_eq!(
        engine.leaf().source_spans(strong.range.clone()),
        vec![6..15]
    );
    assert!(engine
        .leaf()
        .source_spans(3..7)
        .iter()
        .all(|span| span.end <= source.len()));
}

#[test]
fn unresolved_references_are_dependency_reads_not_rendered_links() {
    let engine = parse("[^1] [missing]", 1, Arc::new(ReferenceTable::new()));
    let unresolved: Vec<_> = engine
        .facts()
        .iter()
        .filter_map(|fact| match &fact.kind {
            FactKind::UnresolvedReference {
                normalized_label, ..
            } => Some(normalized_label.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(unresolved, ["^1", "missing"]);
    assert!(!engine
        .facts()
        .iter()
        .any(|fact| matches!(fact.kind, FactKind::ReferenceLink { .. })));
}

#[test]
fn reference_normalization_is_shared_and_case_folded() {
    let mut references = ReferenceTable::new();
    references.define("Blurry Eyes", 91);
    references.define("Σ", 92);
    references.define("^1", 93);
    references.define("Straẞe", 94);
    references.define("Foo\u{a0}BAR", 95);
    references.define("Foo\u{2003}BAR", 96);
    references.define("Foo\u{b}BAR", 97);
    references.define("Foo\u{c}BAR", 98);
    let engine = parse(
        "[one][ blurry\tEYES ] [Σ][σ] [road][STRASSE] [a][FOO\u{a0}bar] \
         [b][FOO\u{2003}bar] [c][FOO\u{b}bar] [d][FOO\u{c}bar] [^1]",
        1,
        Arc::new(references),
    );
    let resolved: Vec<_> = engine
        .facts()
        .iter()
        .filter_map(|fact| match &fact.kind {
            FactKind::ReferenceLink {
                dependency_id,
                normalized_label,
                ..
            } => Some((*dependency_id, normalized_label.as_str())),
            _ => None,
        })
        .collect();
    assert_eq!(
        resolved,
        [
            (91, "blurry eyes"),
            (92, "σ"),
            (94, "strasse"),
            (95, "foo\u{a0}bar"),
            (96, "foo\u{2003}bar"),
            (97, "foo\u{b}bar"),
            (98, "foo\u{c}bar"),
        ]
    );
    assert!(engine.facts().iter().any(|fact| matches!(
        &fact.kind,
        FactKind::UnresolvedReference {
            normalized_label, ..
        } if normalized_label == "^1"
    )));
}

#[test]
fn projected_crlf_counts_two_raw_label_codepoints_inline() {
    for (ascii, resolves) in [(997, true), (998, false)] {
        let label = "a".repeat(ascii);
        let source = format!("[{label}\r\n]");
        let before_crlf = 1 + ascii;
        let after_crlf = before_crlf + 2;
        let leaf = LogicalLeaf::segmented(
            source.clone(),
            vec![
                Segment::Source(0..before_crlf),
                Segment::ProjectedLineEnding {
                    anchor: after_crlf,
                    raw_codepoints: 2,
                },
                Segment::Source(after_crlf..source.len()),
            ],
        )
        .unwrap();
        let mut references = ReferenceTable::new();
        references.define(&format!("{label}\r\n"), 111);
        let engine = parse_to_completion(leaf, Arc::new(references), 1);
        let resolved = engine.facts().iter().any(|fact| {
            matches!(
                fact.kind,
                FactKind::ReferenceLink {
                    dependency_id: 111,
                    ..
                }
            )
        });
        assert_eq!(resolved, resolves, "ASCII scalars before CRLF={ascii}");
    }
}

#[test]
fn projected_tab_counts_one_raw_label_codepoint_inline() {
    for (ascii, resolves) in [(997, true), (998, false)] {
        let label = "a".repeat(ascii);
        let source = format!("[{label}\tb]");
        let before_tab = 1 + ascii;
        let after_tab = before_tab + 1;
        let leaf = LogicalLeaf::segmented(
            source.clone(),
            vec![
                Segment::Source(0..before_tab),
                Segment::ProjectedTab {
                    anchor: after_tab,
                    spaces: 3,
                },
                Segment::Source(after_tab..source.len()),
            ],
        )
        .unwrap();
        let mut references = ReferenceTable::new();
        references.define(&format!("{label}\tb"), 121);
        let engine = parse_to_completion(leaf, Arc::new(references), 1);
        let resolved = engine.facts().iter().any(|fact| {
            matches!(
                fact.kind,
                FactKind::ReferenceLink {
                    dependency_id: 121,
                    ..
                }
            )
        });
        assert_eq!(
            resolved, resolves,
            "ASCII scalars before projected tab={ascii}"
        );
    }
}

#[test]
fn canonical_duplicate_definitions_are_first_wins() {
    let mut references = ReferenceTable::new();
    references.define("Straẞe\tName", 201);
    references.define("STRASSE NAME", 202);
    references.define_normalized("strasse name".to_owned(), 203);
    let engine = parse("[road][strasse name]", 1, Arc::new(references));
    assert!(engine.facts().iter().any(|fact| matches!(
        fact.kind,
        FactKind::ReferenceLink {
            dependency_id: 201,
            ..
        }
    )));
}

#[test]
fn cancellation_is_checked_inside_a_giant_leaf() {
    let source = "a".repeat(10 * 1024 * 1024);
    let leaf = LogicalLeaf::contiguous(source);
    let cancellation = CancellationToken::default();
    let mut engine = InlineEngine::new(leaf, Arc::new(ReferenceTable::new()));
    assert_eq!(engine.resume(4096, &cancellation).work(), 4096);
    cancellation.cancel();
    assert!(matches!(
        engine.resume(4096, &cancellation),
        ParsePoll::Cancelled { work: 0 }
    ));
}

#[test]
fn plain_text_is_one_run_and_zero_syntax_tokens() {
    let source = "a".repeat(1024 * 1024);
    let engine = parse(&source, 4096, Arc::new(ReferenceTable::new()));
    let receipt = engine.memory_receipt();
    assert_eq!(engine.plain_run_count(), 1);
    assert_eq!(receipt.token_count, 0);
    assert!(receipt.total_retained_auxiliary_bytes < 1024);
}
