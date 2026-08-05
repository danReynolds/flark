use flark_pulldown_derived_core::{
    require_production_ready, ChunkKind, Container, Document, Fuel, HeadingLevel, MarkerKind,
    ParserTask, ProductionGap,
};
use pulldown_cmark::{Event, Parser, Tag};

const FIXTURE: &str = concat!(
    "Title\n",
    "=====\n",
    "\n",
    "> quoted\n",
    "> continued\n",
    "\n",
    "- item\n",
    "  continuation\n",
    "  - nested\n",
    "\n",
    "~~~rust\n",
    "let value = 1;\n",
    "~~~\n",
    "\n",
    "Tail\n",
    "----\n",
);

#[test]
fn donor_shaped_subset_emits_direct_ranges_markers_and_ancestry() {
    let document = Document::parse(FIXTURE, Fuel::bytes(7)).unwrap();
    let chunks = document.chunks();
    assert!(matches!(
        chunks[0].kind,
        ChunkKind::Heading(HeadingLevel::H1)
    ));
    assert_eq!(
        &FIXTURE[chunks[0].source.start as usize..chunks[0].source.end as usize],
        "Title\n=====\n"
    );

    let quote = chunks
        .iter()
        .find(|chunk| {
            document.ancestries()[chunk.ancestry as usize]
                .0
                .contains(&Container::BlockQuote)
                && chunk.kind == ChunkKind::Paragraph
        })
        .unwrap();
    assert_eq!(
        document.ancestries()[quote.ancestry as usize].0,
        vec![Container::BlockQuote]
    );

    let nested = chunks
        .iter()
        .find(|chunk| {
            document.ancestries()[chunk.ancestry as usize].0.len() == 2
                && chunk.kind == ChunkKind::Paragraph
        })
        .unwrap();
    assert!(matches!(
        document.ancestries()[nested.ancestry as usize].0.as_slice(),
        [Container::BulletItem { .. }, Container::BulletItem { .. }]
    ));

    assert!(chunks.iter().any(|chunk| matches!(
        chunk.kind,
        ChunkKind::FenceOpen {
            marker: b'~',
            len: 3
        }
    )));
    assert!(chunks.iter().any(|chunk| chunk.kind == ChunkKind::CodeLine));
    assert!(chunks.iter().any(|chunk| matches!(
        chunk.kind,
        ChunkKind::FenceClose {
            marker: b'~',
            len: 3
        }
    )));

    let facts = document.facts();
    assert_eq!(
        facts
            .iter()
            .filter(|fact| fact.kind == MarkerKind::BlockQuote)
            .count(),
        2
    );
    assert!(facts
        .iter()
        .any(|fact| fact.kind == MarkerKind::Setext(HeadingLevel::H1)));
    assert!(facts
        .iter()
        .any(|fact| fact.kind == MarkerKind::Setext(HeadingLevel::H2)));
    for fact in facts {
        assert!((fact.span.start as usize) < (fact.span.end as usize));
        assert!(fact.span.end as usize <= FIXTURE.len());
    }
}

#[test]
fn selected_semantics_agree_with_pulldown_0134() {
    let cases = [
        ("title\n-----\n", "heading"),
        ("> quote\n", "quote"),
        ("- item\n", "list"),
        ("```rs\ncode\n```\n", "fence"),
    ];
    for (markdown, expected) in cases {
        let document = Document::parse(markdown, Fuel::bytes(3)).unwrap();
        let events: Vec<_> = Parser::new(markdown).collect();
        match expected {
            "heading" => {
                assert!(matches!(document.chunks()[0].kind, ChunkKind::Heading(_)));
                assert!(events
                    .iter()
                    .any(|event| matches!(event, Event::Start(Tag::Heading { .. }))));
            }
            "quote" => {
                assert!(document
                    .ancestries()
                    .iter()
                    .any(|ancestry| ancestry.0 == [Container::BlockQuote]));
                assert!(events
                    .iter()
                    .any(|event| matches!(event, Event::Start(Tag::BlockQuote(_)))));
            }
            "list" => {
                assert!(document.ancestries().iter().any(|ancestry| matches!(
                    ancestry.0.as_slice(),
                    [Container::BulletItem { .. }]
                )));
                assert!(events
                    .iter()
                    .any(|event| matches!(event, Event::Start(Tag::List(None)))));
            }
            "fence" => {
                assert!(document
                    .chunks()
                    .iter()
                    .any(|chunk| matches!(chunk.kind, ChunkKind::FenceOpen { .. })));
                assert!(events
                    .iter()
                    .any(|event| matches!(event, Event::Start(Tag::CodeBlock(_)))));
            }
            _ => unreachable!(),
        }
    }
}

#[test]
fn work_budget_does_not_change_clean_semantics() {
    let one = Document::parse(FIXTURE, Fuel::bytes(1)).unwrap();
    let large = Document::parse(FIXTURE, Fuel::bytes(16 * 1024)).unwrap();
    assert_eq!(one.semantic_snapshot(), large.semantic_snapshot());
    assert_eq!(one.memory_receipt().max_advance_source_bytes, 1);
}

#[test]
fn ten_megabyte_physical_line_yields_with_constant_transient_state() {
    let markdown = format!("{}\n", "x".repeat(10_000_000));
    let mut task = ParserTask::new(markdown).unwrap();
    let mut calls = 0;
    let mut max_examined = 0;
    loop {
        let receipt = task.advance(Fuel::bytes(4096)).unwrap();
        calls += 1;
        max_examined = max_examined.max(receipt.source_bytes);
        assert!(receipt.source_bytes <= 4096);
        if receipt.complete {
            break;
        }
    }
    assert!(calls >= 2_442, "calls={calls}");
    assert_eq!(max_examined, 4096);

    let document =
        Document::parse(format!("{}\n", "x".repeat(10_000_000)), Fuel::bytes(4096)).unwrap();
    let memory = document.memory_receipt();
    assert_eq!(memory.chunks, 1);
    assert_eq!(memory.checkpoints, 2);
    assert!(memory.transient_state_bytes <= 2048, "{memory:?}");
}

#[test]
fn million_line_paragraph_uses_sparse_checkpoints_not_nodes_per_line() {
    let markdown = "a\n".repeat(1_000_000);
    let document = Document::parse(markdown, Fuel::bytes(64 * 1024)).unwrap();
    let memory = document.memory_receipt();
    assert_eq!(memory.chunks, 1, "{memory:?}");
    assert_eq!(memory.checkpoints, 2, "{memory:?}");
    let parser_metadata = memory.chunk_bytes
        + memory.fact_bytes
        + memory.checkpoint_bytes
        + memory.checkpoint_container_bytes
        + memory.ancestry_bytes
        + memory.transient_state_bytes;
    assert!(
        parser_metadata < 32 * 1024,
        "metadata={parser_metadata} {memory:?}"
    );
}

#[test]
fn resumed_edits_match_clean_parse_exactly_across_history() {
    let mut markdown = String::new();
    for section in 0..1_200 {
        markdown.push_str(&format!(
            "Section {section:04} word-{section:04}\n----\n\n> quote {section:04}\n\n- item {section:04}\n\n```txt\ncode {section:04}\n```\n\n"
        ));
    }
    let mut document = Document::parse(markdown, Fuel::bytes(1024)).unwrap();
    let mut converged = 0;
    let mut reused = 0;
    for edit in 0..250 {
        let section = (edit * 37) % 1_200;
        let needle = format!("word-{section:04}");
        let start = document.source().find(&needle).unwrap() + 5;
        let replacement = if edit % 3 == 0 { "X" } else { "YY" };
        let receipt = document
            .apply_edit(start..start + 1, replacement, Fuel::bytes(257))
            .unwrap();
        converged += usize::from(receipt.converged);
        reused += receipt.reused_suffix_chunks;
        let clean = Document::parse(document.source(), Fuel::bytes(8192)).unwrap();
        assert_eq!(
            document.semantic_snapshot(),
            clean.semantic_snapshot(),
            "edit={edit} section={section} receipt={receipt:?}"
        );
    }
    assert!(converged >= 240, "converged={converged}");
    assert!(reused > 100_000, "reused={reused}");
}

#[test]
fn production_gaps_are_executable_not_implicit() {
    let gaps = require_production_ready().unwrap_err();
    assert!(gaps.contains(&ProductionGap::PersistentChunkedSource));
    assert!(gaps.contains(&ProductionGap::PersistentOutputTreeAndLazySuffixShift));
    assert!(gaps.contains(&ProductionGap::IntraLineEditCheckpoint));
    assert!(gaps.contains(&ProductionGap::InlineGrammarAndReferenceDependencies));
}
