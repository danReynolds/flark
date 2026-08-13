use flark_runtime::{
    DocumentBulletMarker, DocumentCodeBlockStyle, DocumentFenceCharacter, DocumentHeadingStyle,
    DocumentInlineFactKind, DocumentListDelimiter, DocumentListMarker, DocumentLiveViewportSpan,
    DocumentSession, DocumentSessionPhase, DocumentViewportRowEditCapability,
    DocumentViewportRowPresentation, DOCUMENT_INLINE_FACT_CONTINUITY_PLAIN_TEXT,
};

fn pump_ready(document: &mut DocumentSession) -> usize {
    let mut work = 0;
    while document.phase() != DocumentSessionPhase::Ready {
        let receipt = document.pump(64).expect("parser pump");
        assert!(receipt.work_units <= 64);
        work += receipt.work_units;
        assert!(work < 1_000_000, "small fixture should converge");
    }
    work
}

#[test]
fn open_edit_and_viewport_use_the_new_persistent_runtime() {
    let source = "# Flark\n\nA quick paragraph.\n\n- one\n- two\n";
    let mut document = DocumentSession::begin(source).expect("begin document");
    assert_eq!(document.revision(), 1);
    assert_eq!(document.phase(), DocumentSessionPhase::Building);
    assert!(pump_ready(&mut document) > 0);

    let viewport = document
        .query_viewport(1, 0..source.len(), 32)
        .expect("initial viewport");
    assert!(!viewport.rows.is_empty());
    assert_eq!(viewport.revision, 1);
    assert_eq!(viewport.rows[0].kind, 12, "ATX heading kind");

    let quick = source.find("quick").expect("quick offset");
    let edit = document
        .apply_edit(1, quick..quick + "quick".len(), "fast")
        .expect("local edit");
    assert_eq!(edit.revision, 2);
    assert_eq!(document.phase(), DocumentSessionPhase::Building);
    pump_ready(&mut document);

    let current = String::from_utf8(
        document
            .source_bytes(0..document.source_byte_len())
            .expect("source bytes"),
    )
    .expect("UTF-8 source");
    assert_eq!(current, source.replacen("quick", "fast", 1));
    let viewport = document
        .query_viewport(2, 0..current.len(), 32)
        .expect("edited viewport");
    assert_eq!(viewport.revision, 2);
    assert_eq!(viewport.rows[0].kind, 12);
    document.close().expect("close document");
}

#[test]
fn viewport_preserves_parser_authored_heading_level_and_style() {
    let source = "# One\n\n### Three\n\nSetext one\n===\n\nSetext two\n---\n";
    let mut document = DocumentSession::begin(source).expect("begin document");
    pump_ready(&mut document);
    let viewport = document
        .query_viewport(1, 0..source.len(), 32)
        .expect("heading viewport");
    let headings = viewport
        .rows
        .iter()
        .filter_map(|row| match row.presentation {
            DocumentViewportRowPresentation::Heading { level, style } => Some((level, style)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        headings,
        vec![
            (1, DocumentHeadingStyle::Atx),
            (3, DocumentHeadingStyle::Atx),
            (1, DocumentHeadingStyle::Setext),
            (2, DocumentHeadingStyle::Setext),
        ]
    );
    document.close().expect("close document");
}

#[test]
fn viewport_exposes_an_empty_atx_heading_caret() {
    let source = "# \n";
    let mut document = DocumentSession::begin(source).expect("begin empty heading");
    pump_ready(&mut document);
    let viewport = document
        .query_viewport(1, 0..source.len(), 8)
        .expect("empty heading viewport");
    let row = viewport.rows.first().expect("empty heading row");
    assert_eq!(
        row.presentation,
        DocumentViewportRowPresentation::Heading {
            level: 1,
            style: DocumentHeadingStyle::Atx,
        }
    );
    assert_eq!(row.editable_range, Some(2..2));
    assert_eq!(row.editable_utf16_range, Some(2..2));
    document.close().expect("close empty heading");
}

#[test]
fn viewport_preserves_parser_authored_list_markers_and_prefix_geometry() {
    let source = "- alpha\n9) beta\n42) ";
    let mut document = DocumentSession::begin(source).expect("begin document");
    pump_ready(&mut document);
    let viewport = document
        .query_viewport(1, 0..source.len(), 32)
        .expect("list viewport");
    let lists = viewport
        .rows
        .iter()
        .filter_map(|row| match row.presentation {
            DocumentViewportRowPresentation::ListItem {
                marker,
                prefix_start_byte,
                prefix_end_byte,
                prefix_start_utf16,
                prefix_end_utf16,
                nesting_depth,
                marker_offset,
                simple_continuation,
                starts_list,
                task_checked,
                ..
            } => Some((
                row.kind,
                marker,
                prefix_start_byte,
                prefix_end_byte,
                prefix_start_utf16,
                prefix_end_utf16,
                row.source_range.start,
                nesting_depth,
                marker_offset,
                simple_continuation,
                starts_list,
                task_checked,
            )),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(lists.len(), 3);
    assert_eq!(lists[0].0, 5);
    assert_eq!(
        lists[0].1,
        DocumentListMarker::Bullet(DocumentBulletMarker::Hyphen)
    );
    assert_eq!(
        (lists[0].2, lists[0].3, lists[0].4, lists[0].5, lists[0].6),
        (0, 2, 0, 2, 2)
    );
    assert_eq!(
        (lists[0].7, lists[0].8, lists[0].9, lists[0].10),
        (1, 0, true, true)
    );
    assert_eq!(
        lists[1].1,
        DocumentListMarker::Ordered {
            value: 9,
            delimiter: DocumentListDelimiter::Parenthesis,
        }
    );
    assert_eq!(lists[2].0, 14, "terminal empty item has a caret row");
    assert_eq!(
        lists[2].1,
        DocumentListMarker::Ordered {
            value: 42,
            delimiter: DocumentListDelimiter::Parenthesis,
        }
    );
    assert!(!lists[2].10, "later item does not start its ordered List");
    assert!(lists.iter().all(|row| row.11.is_none()));
    document.close().expect("close document");

    let nested_source = "- parent\n  - child\n";
    let mut nested = DocumentSession::begin(nested_source).expect("begin nested List");
    pump_ready(&mut nested);
    let viewport = nested
        .query_viewport(1, 0..nested_source.len(), 16)
        .expect("nested List viewport");
    let child = viewport
        .rows
        .iter()
        .find(|row| row.source_range.start == 13)
        .expect("nested child row");
    assert_eq!(child.editable_range, Some(13..18));
    assert_eq!(
        child.presentation,
        DocumentViewportRowPresentation::ListItem {
            marker: DocumentListMarker::Bullet(DocumentBulletMarker::Hyphen),
            prefix_start_byte: 11,
            prefix_end_byte: 13,
            prefix_start_utf16: 11,
            prefix_end_utf16: 13,
            nesting_depth: 2,
            marker_offset: 0,
            container_widths: 2,
            container_count: 1,
            marker_column: 2,
            simple_continuation: true,
            starts_list: true,
            task_checked: None,
        }
    );
    nested.close().expect("close nested List");

    let empty_nested_source = "- parent\n  - child\n  - \n";
    let mut empty_nested =
        DocumentSession::begin(empty_nested_source).expect("begin empty nested List");
    pump_ready(&mut empty_nested);
    let viewport = empty_nested
        .query_viewport(1, 0..empty_nested_source.len(), 16)
        .expect("empty nested List viewport");
    assert!(
        matches!(
            viewport.rows.last().expect("empty nested row").presentation,
            DocumentViewportRowPresentation::ListItem {
                nesting_depth: 2,
                simple_continuation: true,
                ..
            }
        ),
        "{:#?}",
        viewport.rows
    );
    empty_nested.close().expect("close empty nested List");

    let depth_three_source = "- root\n  - child\n    - leaf\n";
    let mut depth_three =
        DocumentSession::begin(depth_three_source).expect("begin depth-three List");
    pump_ready(&mut depth_three);
    let viewport = depth_three
        .query_viewport(1, 0..depth_three_source.len(), 16)
        .expect("depth-three List viewport");
    let leaf = viewport.rows.last().expect("depth-three leaf row");
    assert!(matches!(
        leaf.presentation,
        DocumentViewportRowPresentation::ListItem {
            nesting_depth: 3,
            simple_continuation: true,
            ..
        }
    ));
    depth_three.close().expect("close depth-three List");

    let nonuniform_source = "10. root\n    - child\n";
    let mut nonuniform =
        DocumentSession::begin(nonuniform_source).expect("begin nonuniform nested List");
    pump_ready(&mut nonuniform);
    let viewport = nonuniform
        .query_viewport(1, 0..nonuniform_source.len(), 16)
        .expect("nonuniform nested List viewport");
    let child = viewport.rows.last().expect("nonuniform child row");
    assert!(matches!(
        child.presentation,
        DocumentViewportRowPresentation::ListItem {
            nesting_depth: 2,
            marker_offset: 0,
            container_widths: 4,
            container_count: 1,
            marker_column: 4,
            simple_continuation: true,
            ..
        }
    ));
    nonuniform.close().expect("close nonuniform nested List");

    let offset_nonuniform_source = "10. root\n     - child\n";
    let mut offset_nonuniform = DocumentSession::begin(offset_nonuniform_source)
        .expect("begin offset nonuniform nested List");
    pump_ready(&mut offset_nonuniform);
    let viewport = offset_nonuniform
        .query_viewport(1, 0..offset_nonuniform_source.len(), 16)
        .expect("offset nonuniform nested List viewport");
    let child = viewport.rows.last().expect("offset nonuniform child row");
    assert!(matches!(
        child.presentation,
        DocumentViewportRowPresentation::ListItem {
            nesting_depth: 2,
            marker_offset: 1,
            container_widths: 4,
            container_count: 1,
            marker_column: 5,
            simple_continuation: true,
            ..
        }
    ));
    offset_nonuniform
        .close()
        .expect("close offset nonuniform nested List");

    let continued_source = "9) alpha\n10) \n";
    let mut continued = DocumentSession::begin(continued_source).expect("begin continued List");
    pump_ready(&mut continued);
    let viewport = continued
        .query_viewport(1, 0..continued_source.len(), 32)
        .expect("continued List viewport");
    let terminal = viewport
        .rows
        .iter()
        .find(|row| row.kind == 14)
        .expect("terminal empty List Item row");
    assert_eq!(terminal.source_range, 14..14);
    assert_eq!(terminal.editable_range, Some(13..13));
    assert_eq!(terminal.editable_utf16_range, Some(13..13));
    match terminal.presentation {
        DocumentViewportRowPresentation::ListItem {
            marker:
                DocumentListMarker::Ordered {
                    value: 10,
                    delimiter: DocumentListDelimiter::Parenthesis,
                },
            prefix_start_byte: 9,
            prefix_end_byte: 13,
            prefix_start_utf16: 9,
            prefix_end_utf16: 13,
            ..
        } => {}
        other => panic!("unexpected terminal List presentation: {other:?}"),
    }
    continued.close().expect("close continued List");
}

#[test]
fn viewport_exposes_gfm_task_state_without_reparsing_source() {
    let source = "- [ ] foo\n- [X] bar\n";
    let mut document = DocumentSession::begin(source).expect("begin task document");
    pump_ready(&mut document);
    let viewport = document
        .query_viewport(1, 0..source.len(), 32)
        .expect("task viewport");
    let tasks = viewport
        .rows
        .iter()
        .filter_map(|row| match row.presentation {
            DocumentViewportRowPresentation::ListItem { task_checked, .. } => {
                Some((task_checked, row.source_range.clone()))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(tasks, vec![(Some(false), 6..10), (Some(true), 16..20)]);
    document.close().expect("close task document");
}

#[test]
fn viewport_preserves_parser_authored_block_structure_presentations() {
    let quote_source = "> quote\n";
    let mut quote = DocumentSession::begin(quote_source).expect("begin BlockQuote");
    pump_ready(&mut quote);
    let viewport = quote
        .query_viewport(1, 0..quote_source.len(), 32)
        .expect("BlockQuote viewport");
    assert_eq!(viewport.rows.len(), 1);
    assert_eq!(viewport.rows[0].editable_range, Some(2..7));
    assert_eq!(
        viewport.rows[0].presentation,
        DocumentViewportRowPresentation::BlockQuote {
            prefix_start_byte: 0,
            prefix_end_byte: 2,
            prefix_start_utf16: 0,
            prefix_end_utf16: 2,
            nesting_depth: 1,
            simple_continuation: true,
        }
    );
    quote.close().expect("close BlockQuote");

    let unterminated_source = "> alpha";
    let mut unterminated =
        DocumentSession::begin(unterminated_source).expect("begin unterminated BlockQuote");
    pump_ready(&mut unterminated);
    let unterminated_viewport = unterminated
        .query_viewport(1, 0..unterminated_source.len(), 32)
        .expect("unterminated BlockQuote viewport");
    assert_eq!(unterminated_viewport.rows.len(), 1);
    assert_eq!(unterminated_viewport.rows[0].editable_range, Some(2..7));
    unterminated.close().expect("close unterminated BlockQuote");

    let nested_source = "> > nested\n";
    let mut nested = DocumentSession::begin(nested_source).expect("begin nested BlockQuote");
    pump_ready(&mut nested);
    let viewport = nested
        .query_viewport(1, 0..nested_source.len(), 32)
        .expect("nested BlockQuote viewport");
    assert_eq!(
        viewport.rows[0].presentation,
        DocumentViewportRowPresentation::BlockQuote {
            prefix_start_byte: 0,
            prefix_end_byte: 4,
            prefix_start_utf16: 0,
            prefix_end_utf16: 4,
            nesting_depth: 2,
            simple_continuation: false,
        }
    );
    nested.close().expect("close nested BlockQuote");

    let multiline_source = "> first\n> second\n";
    let mut multiline =
        DocumentSession::begin(multiline_source).expect("begin multiline BlockQuote");
    pump_ready(&mut multiline);
    let viewport = multiline
        .query_viewport(1, 0..multiline_source.len(), 32)
        .expect("multiline BlockQuote viewport");
    assert!(matches!(
        viewport.rows[0].presentation,
        DocumentViewportRowPresentation::BlockQuote {
            simple_continuation: false,
            ..
        }
    ));
    assert_eq!(
        viewport.rows[0].edit_capability,
        DocumentViewportRowEditCapability::ProjectedReserved,
    );
    assert_eq!(viewport.rows[0].editable_range, Some(2..16));
    assert_eq!(viewport.rows[0].editable_utf16_range, Some(2..16));
    let segments = viewport.rows[0]
        .projection_segments
        .as_ref()
        .expect("multiline quote projection segments");
    assert_eq!(
        segments
            .iter()
            .map(|segment| segment.source_range.clone())
            .collect::<Vec<_>>(),
        vec![2..8, 10..16],
    );
    assert_eq!(
        segments
            .iter()
            .map(|segment| segment.source_utf16_range.clone())
            .collect::<Vec<_>>(),
        vec![2..8, 10..16],
    );
    multiline.close().expect("close multiline BlockQuote");

    let empty_multiline_source = "> first\n> \n";
    let mut empty_multiline =
        DocumentSession::begin(empty_multiline_source).expect("begin empty multiline BlockQuote");
    pump_ready(&mut empty_multiline);
    let empty_multiline_viewport = empty_multiline
        .query_viewport(1, 0..empty_multiline_source.len(), 32)
        .expect("empty multiline BlockQuote viewport");
    assert_eq!(empty_multiline_viewport.rows.len(), 2);
    let empty_quote = &empty_multiline_viewport.rows[1];
    assert_eq!(empty_quote.kind, 15);
    assert_eq!(empty_quote.source_range, 11..11);
    assert_eq!(empty_quote.editable_range, Some(10..10));
    assert_eq!(empty_quote.editable_utf16_range, Some(10..10));
    assert_eq!(
        empty_quote.presentation,
        DocumentViewportRowPresentation::BlockQuote {
            prefix_start_byte: 8,
            prefix_end_byte: 10,
            prefix_start_utf16: 8,
            prefix_end_utf16: 10,
            nesting_depth: 1,
            simple_continuation: true,
        }
    );
    empty_multiline
        .close()
        .expect("close empty multiline BlockQuote");

    let long_empty_source = format!("> {}\n> \n", "🌍".repeat(32));
    let prefix_start_byte = long_empty_source.len() - 3;
    let prefix_end_byte = long_empty_source.len() - 1;
    let prefix_start_utf16 = long_empty_source[..prefix_start_byte]
        .encode_utf16()
        .count();
    let prefix_end_utf16 = long_empty_source[..prefix_end_byte].encode_utf16().count();
    let mut long_empty =
        DocumentSession::begin(&long_empty_source).expect("begin long empty multiline BlockQuote");
    pump_ready(&mut long_empty);
    let long_empty_viewport = long_empty
        .query_viewport(1, 0..long_empty_source.len(), 32)
        .expect("long empty multiline BlockQuote viewport");
    let long_empty_quote = long_empty_viewport
        .rows
        .last()
        .expect("long empty quote row");
    assert_eq!(long_empty_quote.kind, 15);
    assert_eq!(
        long_empty_quote.editable_range,
        Some(prefix_end_byte as u64..prefix_end_byte as u64),
    );
    assert_eq!(
        long_empty_quote.editable_utf16_range,
        Some(prefix_end_utf16 as u64..prefix_end_utf16 as u64),
    );
    assert_eq!(
        long_empty_quote.presentation,
        DocumentViewportRowPresentation::BlockQuote {
            prefix_start_byte: prefix_start_byte as u64,
            prefix_end_byte: prefix_end_byte as u64,
            prefix_start_utf16: prefix_start_utf16 as u64,
            prefix_end_utf16: prefix_end_utf16 as u64,
            nesting_depth: 1,
            simple_continuation: true,
        },
    );
    long_empty
        .close()
        .expect("close long empty multiline BlockQuote");

    let nested_multiline_source = "> > first\n> > second\n";
    let mut nested_multiline =
        DocumentSession::begin(nested_multiline_source).expect("begin nested multiline quote");
    pump_ready(&mut nested_multiline);
    let nested_multiline_viewport = nested_multiline
        .query_viewport(1, 0..nested_multiline_source.len(), 32)
        .expect("nested multiline quote viewport");
    assert_eq!(
        nested_multiline_viewport.rows[0].edit_capability,
        DocumentViewportRowEditCapability::Unavailable,
    );
    assert!(nested_multiline_viewport.rows[0]
        .projection_segments
        .is_none());
    nested_multiline
        .close()
        .expect("close nested multiline quote");

    let fenced_source = "```dart\ncode\n```\n";
    let mut fenced = DocumentSession::begin(fenced_source).expect("begin FencedCode");
    pump_ready(&mut fenced);
    let viewport = fenced
        .query_viewport(1, 0..fenced_source.len(), 32)
        .expect("FencedCode viewport");
    assert_eq!(viewport.rows[0].editable_range, Some(8..13));
    assert_eq!(
        viewport.rows[0].presentation,
        DocumentViewportRowPresentation::CodeBlock {
            style: DocumentCodeBlockStyle::Fenced {
                fence: DocumentFenceCharacter::Backtick,
                minimum_closing_length: 3,
                fence_offset: 0,
                closed: true,
            },
        }
    );
    fenced.close().expect("close FencedCode");

    let indented_source = "    code\n    more\n";
    let mut indented = DocumentSession::begin(indented_source).expect("begin IndentedCode");
    pump_ready(&mut indented);
    let viewport = indented
        .query_viewport(1, 0..indented_source.len(), 32)
        .expect("IndentedCode viewport");
    assert_eq!(
        viewport.rows[0].presentation,
        DocumentViewportRowPresentation::CodeBlock {
            style: DocumentCodeBlockStyle::Indented,
        }
    );
    assert_eq!(
        viewport.rows[0].edit_capability,
        DocumentViewportRowEditCapability::ProjectedReserved,
    );
    assert_eq!(
        viewport.rows[0]
            .projection_segments
            .as_ref()
            .expect("indented code projection segments")
            .iter()
            .map(|segment| segment.source_range.clone())
            .collect::<Vec<_>>(),
        vec![4..9, 13..18],
    );
    indented.close().expect("close IndentedCode");

    let thematic_source = "---\n";
    let mut thematic = DocumentSession::begin(thematic_source).expect("begin ThematicBreak");
    pump_ready(&mut thematic);
    let viewport = thematic
        .query_viewport(1, 0..thematic_source.len(), 32)
        .expect("ThematicBreak viewport");
    assert_eq!(
        viewport.rows[0].presentation,
        DocumentViewportRowPresentation::ThematicBreak
    );
    thematic.close().expect("close ThematicBreak");

    for empty_source in ["> ", "> \n"] {
        let mut empty = DocumentSession::begin(empty_source).expect("begin empty BlockQuote");
        pump_ready(&mut empty);
        let viewport = empty
            .query_viewport(1, 0..empty_source.len(), 32)
            .expect("empty BlockQuote viewport");
        assert_eq!(viewport.rows.len(), 1);
        assert_eq!(viewport.rows[0].kind, 15);
        assert_eq!(
            viewport.rows[0].source_range,
            empty_source.len() as u64..empty_source.len() as u64,
        );
        assert_eq!(viewport.rows[0].editable_range, Some(2..2));
        assert_eq!(
            viewport.rows[0].presentation,
            DocumentViewportRowPresentation::BlockQuote {
                prefix_start_byte: 0,
                prefix_end_byte: 2,
                prefix_start_utf16: 0,
                prefix_end_utf16: 2,
                nesting_depth: 1,
                simple_continuation: true,
            }
        );
        empty.close().expect("close empty BlockQuote");
    }
}

#[test]
fn viewport_carries_complete_parser_authored_inline_geometry_or_fails_closed() {
    let source = "*em* **strong** `code` [link](https://example.com) <https://a.test>\n";
    let mut document = DocumentSession::begin(source).expect("begin inline document");
    pump_ready(&mut document);
    let viewport = document
        .query_viewport(1, 0..source.len(), 32)
        .expect("inline viewport");
    let facts = viewport.rows[0]
        .inline_facts
        .as_ref()
        .expect("authoritative inline facts");
    assert_eq!(
        facts.iter().map(|fact| fact.kind).collect::<Vec<_>>(),
        vec![
            DocumentInlineFactKind::Emphasis,
            DocumentInlineFactKind::Strong,
            DocumentInlineFactKind::Code,
            DocumentInlineFactKind::DirectLink,
            DocumentInlineFactKind::AutolinkUri,
        ]
    );
    assert!(facts[..4]
        .iter()
        .all(|fact| fact.flags & DOCUMENT_INLINE_FACT_CONTINUITY_PLAIN_TEXT != 0));
    assert_eq!(
        facts[4].flags & DOCUMENT_INLINE_FACT_CONTINUITY_PLAIN_TEXT,
        0,
        "autolink validity must be recertified"
    );
    for (fact, spelling, content) in [
        (&facts[0], "*em*", "em"),
        (&facts[1], "**strong**", "strong"),
        (&facts[2], "`code`", "code"),
        (&facts[3], "[link](https://example.com)", "link"),
        (&facts[4], "<https://a.test>", "https://a.test"),
    ] {
        let source_start = source.find(spelling).expect("source spelling");
        let content_start = source_start + spelling.find(content).expect("content spelling");
        assert_eq!(
            fact.source_range,
            source_start as u64..(source_start + spelling.len()) as u64
        );
        assert_eq!(
            fact.content_range,
            content_start as u64..(content_start + content.len()) as u64
        );
        assert_eq!(fact.source_utf16_range, fact.source_range);
        assert_eq!(fact.content_utf16_range, fact.content_range);
    }
    document.close().expect("close inline document");

    let transformed_source = "\\* &ngE; ` a ` [ref][id] ![alt](image.png)\n\n[id]: /target\n";
    let mut transformed =
        DocumentSession::begin(transformed_source).expect("begin transformed inline document");
    pump_ready(&mut transformed);
    let viewport = transformed
        .query_viewport(1, 0..transformed_source.len(), 32)
        .expect("transformed inline viewport");
    let facts = viewport.rows[0]
        .inline_facts
        .as_ref()
        .expect("transformed inline facts");
    assert_eq!(
        facts.iter().map(|fact| fact.kind).collect::<Vec<_>>(),
        vec![
            DocumentInlineFactKind::BackslashEscape,
            DocumentInlineFactKind::Replacement,
            DocumentInlineFactKind::Code,
            DocumentInlineFactKind::ReferenceLink,
            DocumentInlineFactKind::DirectImage,
        ]
    );
    let replacement = facts[1].replacement.expect("entity replacement");
    assert_eq!(replacement.first, '≧');
    assert_eq!(replacement.second, Some('\u{338}'));
    assert_eq!(
        &transformed_source
            [facts[2].content_range.start as usize..facts[2].content_range.end as usize],
        "a"
    );
    transformed
        .close()
        .expect("close transformed inline document");

    let code_source = "`a\r\nb`\n";
    let mut code = DocumentSession::begin(code_source).expect("begin transforming code document");
    pump_ready(&mut code);
    let viewport = code
        .query_viewport(1, 0..code_source.len(), 32)
        .expect("transforming code viewport");
    let facts = viewport.rows[0]
        .inline_facts
        .as_ref()
        .expect("transforming code facts");
    assert_eq!(facts[0].kind, DocumentInlineFactKind::Code);
    assert_eq!(facts[1].kind, DocumentInlineFactKind::Replacement);
    assert_eq!(
        facts[1].replacement.expect("line-ending replacement").first,
        ' '
    );
    assert_eq!(
        &code_source[facts[1].source_range.start as usize..facts[1].source_range.end as usize],
        "\r\n"
    );
    code.close().expect("close transforming code document");

    let hard_break_source = "foo\\\nbar\n";
    let mut hard_break =
        DocumentSession::begin(hard_break_source).expect("begin hard-break document");
    pump_ready(&mut hard_break);
    let viewport = hard_break
        .query_viewport(1, 0..hard_break_source.len(), 32)
        .expect("hard-break viewport");
    assert!(viewport.rows[0]
        .inline_facts
        .as_ref()
        .expect("hard-break facts")
        .iter()
        .any(|fact| fact.kind == DocumentInlineFactKind::HardLineBreak));
    hard_break.close().expect("close hard-break document");

    let reference_source = "[label][id]\n\n[id]: /target\n";
    let mut reference = DocumentSession::begin(reference_source).expect("begin reference document");
    pump_ready(&mut reference);
    let viewport = reference
        .query_viewport(1, 0..reference_source.len(), 32)
        .expect("reference viewport");
    let facts = viewport.rows[0]
        .inline_facts
        .as_ref()
        .expect("reference facts");
    assert_eq!(facts.len(), 1);
    assert_eq!(facts[0].kind, DocumentInlineFactKind::ReferenceLink);
    reference.close().expect("close reference document");
}

#[test]
fn live_viewport_mixes_only_authenticated_rows_with_pending_source() {
    let source = (0..240)
        .map(|index| format!("Paragraph {index:03} has stable source for restart coverage.\n\n"))
        .collect::<String>();
    let mut document = DocumentSession::begin(&source).expect("begin document");
    pump_ready(&mut document);
    let edit_start = source.find("Paragraph 120").expect("middle paragraph");
    document
        .apply_edit(1, edit_start..edit_start + "Paragraph".len(), "Heading")
        .expect("local structural edit");

    let window_start = 0;
    let window_end = (edit_start + 2048).min(document.source_byte_len());
    let initial = document
        .query_live_viewport(2, window_start..window_end, 256)
        .expect("initial mixed viewport");
    assert!(initial
        .spans
        .iter()
        .any(|span| { matches!(span, DocumentLiveViewportSpan::CertifiedUnchanged { .. }) }));
    assert!(initial
        .spans
        .iter()
        .any(|span| { matches!(span, DocumentLiveViewportSpan::Pending { .. }) }));

    let mut saw_certified_suffix_after_pending = false;
    for _ in 0..10_000 {
        if document.phase() == DocumentSessionPhase::Ready {
            break;
        }
        document.pump(1).expect("bounded parser pump");
        let live = document
            .query_live_viewport(2, window_start..window_end, 256)
            .expect("progressive mixed viewport");
        let mut saw_pending = false;
        for span in live.spans {
            match span {
                DocumentLiveViewportSpan::Pending { .. } => saw_pending = true,
                DocumentLiveViewportSpan::CertifiedUnchanged { .. } if saw_pending => {
                    saw_certified_suffix_after_pending = true;
                    break;
                }
                DocumentLiveViewportSpan::CertifiedUnchanged { .. } => {}
            }
        }
        if saw_certified_suffix_after_pending {
            break;
        }
    }
    assert!(
        saw_certified_suffix_after_pending,
        "the converged unchanged suffix becomes current before adoption cleanup"
    );
    document.close().expect("close document");
}

#[test]
fn utf16_and_utf8_coordinates_remain_explicit() {
    let mut document = DocumentSession::begin("a😀b\n").expect("begin document");
    pump_ready(&mut document);
    assert_eq!(document.byte_offset_for_utf16(3).expect("UTF-16 map"), 5);
    assert_eq!(document.utf16_offset_for_byte(5).expect("UTF-8 map"), 3);
    document.close().expect("close document");
}

#[test]
fn edits_supersede_clean_and_incremental_work_without_waiting_for_certification() {
    let source = "## Section\n\nalpha gamma paragraph.\n\n".repeat(128);
    let alpha = source.find("alpha").expect("alpha offset");
    let gamma = source.find("gamma").expect("gamma offset");
    let mut document = DocumentSession::begin(&source).expect("begin document");

    assert_eq!(
        document
            .apply_edit(1, alpha..alpha + 5, "bravo")
            .expect("edit during clean build")
            .revision,
        2
    );
    assert_eq!(
        document
            .apply_edit(2, gamma..gamma + 5, "delta")
            .expect("second edit during clean cancellation")
            .revision,
        3
    );
    pump_ready(&mut document);

    assert_eq!(
        document
            .apply_edit(3, alpha..alpha + 5, "cider")
            .expect("incremental edit")
            .revision,
        4
    );
    assert_eq!(
        document
            .apply_edit(4, gamma..gamma + 5, "eagle")
            .expect("edit superseding incremental adoption")
            .revision,
        5
    );
    pump_ready(&mut document);

    let current = String::from_utf8(
        document
            .source_bytes(0..document.source_byte_len())
            .expect("current source"),
    )
    .expect("UTF-8 source");
    assert!(current.starts_with("## Section\n\ncider eagle paragraph."));
    document.close().expect("close document");
}

#[test]
fn close_cancels_initial_work_and_reclaims_in_bounded_turns() {
    let source = "## Section\n\nA paragraph with **markup**.\n\n".repeat(512);
    let mut document = DocumentSession::begin(&source).expect("begin document");

    document.begin_close().expect("begin bounded close");
    assert_eq!(document.phase(), DocumentSessionPhase::Closing);

    let mut turns = 0;
    loop {
        let receipt = document.pump_close(1).expect("close pump");
        assert!(receipt.work_units <= 1);
        turns += 1;
        if receipt.complete {
            break;
        }
        assert!(turns < 1_000_000, "bounded close should converge");
    }
    assert_eq!(document.phase(), DocumentSessionPhase::Closed);
}

#[test]
fn burst_edits_report_backpressure_and_resume_after_bounded_maintenance() {
    let source = "## Section\n\nA paragraph with **markup**.\n\n".repeat(128);
    let mut document = DocumentSession::begin(&source).expect("begin document");
    let mut admitted = 0;

    loop {
        match document.apply_edit(document.revision(), 0..0, "x") {
            Ok(_) => admitted += 1,
            Err(error) if error.is_backpressure() => break,
            Err(error) => panic!("unexpected burst edit failure: {error:?}"),
        }
        assert!(admitted < 32, "the bounded retirement queue should fill");
    }
    assert!(admitted > 0);

    let maintenance = document.pump(64).expect("bounded maintenance pump");
    assert!(maintenance.work_units <= 64);
    document
        .apply_edit(document.revision(), 0..0, "y")
        .expect("edit after bounded maintenance");
    document.close().expect("close document");
}

#[test]
fn certified_burst_edits_resume_through_bounded_backpressure() {
    let source = "## Section\n\nA quick paragraph with **markup**.\n\n".repeat(128);
    let mut document = DocumentSession::begin(&source).expect("begin document");
    pump_ready(&mut document);
    let offset = source.find("quick").expect("edit offset");

    for index in 0..120 {
        let expected = document.revision();
        match document.apply_edit(expected, offset + index..offset + index, "x") {
            Ok(_) => {}
            Err(error) if error.is_backpressure() => {
                let maintenance = document.pump(512).expect("bounded maintenance pump");
                assert!(maintenance.work_units <= 512);
                document
                    .apply_edit(expected, offset + index..offset + index, "x")
                    .unwrap_or_else(|error| panic!("retry {index} failed: {error:?}"));
            }
            Err(error) => panic!("burst edit {index} failed: {error:?}"),
        }
    }
    document.close().expect("close document");
}
