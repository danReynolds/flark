use comrak::{markdown_to_html, Options as ComrakOptions};
use flark_engine::{SourceStore, SOURCE_CURSOR_WINDOW_BYTES};
use flark_parser::{
    M11BlockQuoteDisposition, M11BlockQuoteLineKind, M11BlockQuoteLineMapping,
    M11BlockQuoteParagraphMapping, M11BlockQuoteUnsupportedReason, M11BulletListItemMapping,
    M11BulletListParagraphMapping, M11CleanBlockController, M11CleanControllerFault,
    M11CleanDocumentKind, M11CleanDocumentResult, M11CleanLeaf, M11ExactController,
    M11ListUnsupportedReason, M11SourceLinePollStatus, M11SourceLineSource, M11UnknownReason,
    M11UnsupportedOpener, SnapshotLinePoll, SnapshotLineScanner, SnapshotLineSource,
    M11_SEGMENTED_LINE_PREFIX_BYTES,
};
use std::ops::Range;

fn ascii_paragraph(
    source: Range<u32>,
    inline_source: Range<u32>,
    reference_definition_count: usize,
) -> M11CleanLeaf {
    M11CleanLeaf::Paragraph {
        source_utf16: source.clone(),
        source,
        inline_source,
        reference_definition_count,
    }
}

fn ascii_blank(source: Range<u32>) -> M11CleanLeaf {
    M11CleanLeaf::Blank {
        source_utf16: source.clone(),
        source,
    }
}

fn ascii_atx_heading(
    source: Range<u32>,
    opening_marker: Range<u32>,
    inline_source: Range<u32>,
    closing_marker: Option<Range<u32>>,
    line_ending: Range<u32>,
    level: u8,
    opening_indent: u8,
) -> M11CleanLeaf {
    M11CleanLeaf::AtxHeading {
        source_utf16: source.clone(),
        source,
        opening_marker,
        inline_source,
        closing_marker,
        line_ending,
        level,
        opening_indent,
        has_bof_bom: false,
    }
}

fn ascii_setext_heading(
    source: Range<u32>,
    inline_source: Range<u32>,
    underline_marker: Range<u32>,
    underline_line_ending: Range<u32>,
    level: u8,
    opening_indent: u8,
    reference_definition_count: usize,
) -> M11CleanLeaf {
    M11CleanLeaf::SetextHeading {
        source_utf16: source.clone(),
        source,
        inline_source,
        underline_marker,
        underline_line_ending,
        level,
        opening_indent,
        reference_definition_count,
    }
}

fn ascii_thematic_break(
    source: Range<u32>,
    marker: u8,
    marker_count: u32,
    marker_envelope: Range<u32>,
    line_ending: Range<u32>,
    opening_indent: u8,
) -> M11CleanLeaf {
    M11CleanLeaf::ThematicBreak {
        source_utf16: source.clone(),
        source,
        marker,
        marker_count,
        marker_envelope,
        line_ending,
        opening_indent,
        has_bof_bom: false,
    }
}

fn ascii_indented_code(
    source: Range<u32>,
    line_count: u32,
    projected_utf8_length: u32,
    terminal_eol_bytes: u32,
) -> M11CleanLeaf {
    M11CleanLeaf::IndentedCode {
        source_utf16: source.clone(),
        source,
        line_count,
        projected_utf8_length,
        projected_utf16_length: projected_utf8_length,
        terminal_eol_bytes,
        has_bof_bom: false,
    }
}

fn ascii_definitions(source: Range<u32>, reference_definition_count: usize) -> M11CleanLeaf {
    M11CleanLeaf::DefinitionsOnly {
        source_utf16: source.clone(),
        source,
        reference_definition_count,
    }
}

fn ascii_unsupported(source: Range<u32>, reason: M11UnknownReason) -> M11CleanLeaf {
    M11CleanLeaf::Unsupported {
        source_utf16: source.clone(),
        source,
        reason,
    }
}

struct ParseReceipt {
    result: M11CleanDocumentResult,
    source_reads: usize,
    max_retained: usize,
    max_work: usize,
}

fn parse(text: &str, fuel: usize, source_grant: usize) -> ParseReceipt {
    let store = SourceStore::new(text).expect("source");
    let mut scanner = SnapshotLineScanner::new(store.snapshot()).expect("scanner");
    let mut controller = M11CleanBlockController::new_for_source(store.version());
    let mut source_reads = 0;
    let mut max_retained = 0;
    let mut max_work = 0;

    loop {
        let line = loop {
            match scanner.poll(fuel).expect("line discovery") {
                SnapshotLinePoll::Pending(next) => scanner = next,
                SnapshotLinePoll::Line(line) => break Some(line),
                SnapshotLinePoll::Complete => break None,
            }
        };
        let Some(line) = line else {
            break;
        };
        let facts = line.facts();
        let mut source = line.into_source().expect("line source");
        let mut admission =
            <M11CleanBlockController as M11ExactController<SnapshotLineSource>>::begin_source_line(
                &mut controller,
                facts.identity(),
            )
            .expect("begin line");

        loop {
            if source.access_budget() == 0 && source.position() < source.len() {
                source
                    .replenish_access_budget(source_grant)
                    .expect("source grant");
            }
            let receipt = <M11CleanBlockController as M11ExactController<
                SnapshotLineSource,
            >>::poll_source_line(&mut controller, &mut admission, &mut source, fuel)
            .expect("poll line");
            assert!(receipt.lexical_work_units <= fuel);
            assert_eq!(receipt.maximum_source_request_rewind_bytes, 0);
            source_reads += receipt.source_first_reads;
            max_retained = max_retained.max(receipt.retained_source_bytes);
            max_work = max_work.max(receipt.lexical_work_units);
            if receipt.status == M11SourceLinePollStatus::Matched {
                break;
            }
        }

        <M11CleanBlockController as M11ExactController<SnapshotLineSource>>::commit_source_line(
            &mut controller,
            admission,
            facts,
        )
        .expect("commit line");
        scanner = source.finish().expect("line consumed");
    }

    ParseReceipt {
        result: controller
            .finish()
            .unwrap_or_else(|error| panic!("finish document {text:?}: {error:?}")),
        source_reads,
        max_retained,
        max_work,
    }
}

#[test]
fn empty_and_one_root_paragraph_are_exact_terminal_results() {
    let empty = parse("", 1, 1).result;
    assert_eq!(empty.kind(), M11CleanDocumentKind::Empty);
    assert_eq!(empty.source_range(), 0..0);
    assert!(empty.definitions().is_empty());

    for text in [
        "plain paragraph",
        "first line\nsecond line\r\nthird line",
        "\u{feff}  paragraph\ncontinuation",
        "[not a definition\nliteral text",
        "plain\n<x> remains literal inside an open paragraph",
        "plain\n2. does not interrupt a paragraph",
    ] {
        let receipt = parse(text, 3, 2);
        assert_eq!(
            receipt.result.kind(),
            M11CleanDocumentKind::Paragraph,
            "{text:?}: {:?}",
            receipt.result,
        );
        assert_eq!(receipt.source_reads, text.len());
    }
}

#[test]
fn every_competing_root_opener_fails_closed_with_its_typed_winner() {
    let cases = [
        ("<div>", M11UnsupportedOpener::HtmlBlock),
        ("| --- |", M11UnsupportedOpener::TableCandidate),
    ];

    for (text, opener) in cases {
        let result = parse(text, 2, 1).result;
        assert_eq!(
            result.kind(),
            M11CleanDocumentKind::Unknown(M11UnknownReason::UnsupportedOpener(opener)),
            "{text:?}",
        );
        assert_eq!(
            result.source_range(),
            0..u32::try_from(text.len()).expect("short fixture"),
            "{text:?}",
        );
    }
}

#[test]
fn tight_bullet_list_has_exact_list_item_paragraph_and_projection_geometry() {
    let source = "\u{feff}  -  α😀\r\n  - β\r-";
    let result = parse(source, 1, 1).result;
    assert_eq!(result.kind(), M11CleanDocumentKind::Segmented);
    assert_eq!(
        result.leaves(),
        &[M11CleanLeaf::BulletList {
            source: 0..24,
            source_utf16: 0..18,
            marker: b'-',
            items: vec![
                M11BulletListItemMapping {
                    ordinal: 0,
                    source: 0..16,
                    source_utf16: 0..11,
                    opening_marker: 5..6,
                    hidden_prefix: 0..8,
                    hidden_prefix_utf16: 0..6,
                    continuation_prefix_source: 3..8,
                    continuation_prefix_source_utf16: 1..6,
                    content_source: 8..14,
                    content_source_utf16: 6..9,
                    line_ending: 14..16,
                    line_ending_utf16: 9..11,
                    marker: b'-',
                    paragraph: Some(M11BulletListParagraphMapping {
                        source: 8..14,
                        source_utf16: 6..9,
                        inline_source: 8..14,
                        inline_source_utf16: 6..9,
                    }),
                },
                M11BulletListItemMapping {
                    ordinal: 1,
                    source: 16..23,
                    source_utf16: 11..17,
                    opening_marker: 18..19,
                    hidden_prefix: 16..20,
                    hidden_prefix_utf16: 11..15,
                    continuation_prefix_source: 16..20,
                    continuation_prefix_source_utf16: 11..15,
                    content_source: 20..22,
                    content_source_utf16: 15..16,
                    line_ending: 22..23,
                    line_ending_utf16: 16..17,
                    marker: b'-',
                    paragraph: Some(M11BulletListParagraphMapping {
                        source: 20..22,
                        source_utf16: 15..16,
                        inline_source: 20..22,
                        inline_source_utf16: 15..16,
                    }),
                },
                M11BulletListItemMapping {
                    ordinal: 2,
                    source: 23..24,
                    source_utf16: 17..18,
                    opening_marker: 23..24,
                    hidden_prefix: 23..24,
                    hidden_prefix_utf16: 17..18,
                    continuation_prefix_source: 23..24,
                    continuation_prefix_source_utf16: 17..18,
                    content_source: 24..24,
                    content_source_utf16: 18..18,
                    line_ending: 24..24,
                    line_ending_utf16: 18..18,
                    marker: b'-',
                    paragraph: None,
                },
            ]
            .into_boxed_slice(),
            projected_utf8_length: 11,
            projected_utf16_length: 7,
            tight: true,
        }]
    );
}

#[test]
fn tight_bullet_lists_match_comrak_and_marker_changes_form_distinct_lists() {
    let source = "- alpha\n- beta\n+ gamma\n* delta";
    let result = parse(source, 1, 1).result;
    let markers = result
        .leaves()
        .iter()
        .map(|leaf| match leaf {
            M11CleanLeaf::BulletList { marker, items, .. } => (*marker, items.len()),
            other => panic!("unexpected leaf {other:?}"),
        })
        .collect::<Vec<_>>();
    assert_eq!(markers, vec![(b'-', 2), (b'+', 1), (b'*', 1)]);
    assert_eq!(
        markdown_to_html(source, &ComrakOptions::default()),
        "<ul>\n<li>alpha</li>\n<li>beta</li>\n</ul>\n<ul>\n<li>gamma</li>\n</ul>\n<ul>\n<li>delta</li>\n</ul>\n",
    );

    let terminal_empty = "- alpha\n-";
    let terminal_result = parse(terminal_empty, 1, 1).result;
    let [M11CleanLeaf::BulletList { items, .. }] = terminal_result.leaves() else {
        panic!("terminal empty list was not exact");
    };
    assert_eq!(items.len(), 2);
    assert!(items[0].paragraph.is_some());
    assert!(items[1].paragraph.is_none());
    assert_eq!(
        markdown_to_html(terminal_empty, &ComrakOptions::default()),
        "<ul>\n<li>alpha</li>\n<li></li>\n</ul>\n",
    );
}

#[test]
fn tight_ordered_list_preserves_start_delimiter_literal_markers_and_exact_geometry() {
    let source = "007) α😀\r\n9) beta\r\n42) ";
    let result = parse(source, 1, 1).result;
    assert_eq!(result.kind(), M11CleanDocumentKind::Segmented);
    let [M11CleanLeaf::OrderedList {
        source: list_source,
        source_utf16,
        start,
        delimiter,
        items,
        projected_utf8_length,
        projected_utf16_length,
        tight,
    }] = result.leaves()
    else {
        panic!("ordered list was not exact: {:?}", result.leaves());
    };
    assert_eq!(list_source, &(0..26));
    assert_eq!(source_utf16, &(0..23));
    assert_eq!((*start, *delimiter), (7, b')'));
    assert_eq!((*projected_utf8_length, *projected_utf16_length), (14, 11));
    assert!(*tight);
    assert_eq!(items.len(), 3);
    assert_eq!(
        items
            .iter()
            .map(|item| (item.ordinal, item.marker_value, item.delimiter))
            .collect::<Vec<_>>(),
        vec![(0, 7, b')'), (1, 9, b')'), (2, 42, b')')],
    );
    assert_eq!(items[0].opening_marker, 0..4);
    assert_eq!(items[0].hidden_prefix, 0..5);
    assert_eq!(items[0].content_source, 5..11);
    assert_eq!(items[0].line_ending, 11..13);
    assert_eq!(items[1].opening_marker, 13..15);
    assert_eq!(items[1].content_source, 16..20);
    assert_eq!(items[2].opening_marker, 22..25);
    assert_eq!(items[2].continuation_prefix_source, 22..26);
    assert_eq!(items[2].content_source, 26..26);
    assert!(items[0].paragraph.is_some());
    assert!(items[1].paragraph.is_some());
    assert!(items[2].paragraph.is_none());
    assert_eq!(
        markdown_to_html(source, &ComrakOptions::default()),
        "<ol start=\"7\">\n<li>α😀</li>\n<li>beta</li>\n<li></li>\n</ol>\n",
    );
}

#[test]
fn ordered_delimiter_or_list_type_change_closes_the_exact_list() {
    let source = "1. alpha\n8. beta\n3) gamma\n- delta";
    let result = parse(source, 1, 1).result;
    assert!(matches!(
        result.leaves(),
        [
            M11CleanLeaf::OrderedList {
                start: 1,
                delimiter: b'.',
                items,
                ..
            },
            M11CleanLeaf::OrderedList {
                start: 3,
                delimiter: b')',
                ..
            },
            M11CleanLeaf::BulletList { marker: b'-', .. },
        ] if items.len() == 2
    ));
}

#[test]
fn ordered_terminal_empty_accepts_lf_crlf_and_eof_geometry() {
    for (source, expected_eol_bytes) in [
        ("1. alpha\n2. \n", 1),
        ("1) α\r\n2) \r\n", 2),
        ("1. alpha\n2. ", 0),
    ] {
        let result = parse(source, 1, 1).result;
        let [M11CleanLeaf::OrderedList { items, .. }] = result.leaves() else {
            panic!("terminal-empty ordered list was not exact: {source:?}");
        };
        let terminal = items.last().expect("terminal item");
        assert!(terminal.paragraph.is_none(), "{source:?}");
        assert_eq!(
            terminal.line_ending.end - terminal.line_ending.start,
            expected_eol_bytes,
            "{source:?}",
        );
        assert_eq!(terminal.content_source.start, terminal.content_source.end);
    }
}

#[test]
fn ordered_v1_reuses_the_tight_list_fail_closed_boundaries() {
    let cases = [
        ("1. [x] task\n", M11ListUnsupportedReason::Task),
        ("1.\titem\n", M11ListUnsupportedReason::TabPadded),
        ("1.     code\n", M11ListUnsupportedReason::ExcessivePadding),
        ("1. 1. nested\n", M11ListUnsupportedReason::Nested),
        ("1. # heading\n", M11ListUnsupportedReason::BlockChild),
        (
            "1. first\n   continuation\n",
            M11ListUnsupportedReason::LazyOrMultiline,
        ),
        ("1. first\n\n2. second\n", M11ListUnsupportedReason::Loose),
        (
            "1. first\n2. \n3. next\n",
            M11ListUnsupportedReason::NonTerminalEmptyItem,
        ),
    ];
    for (source, reason) in cases {
        let result = parse(source, 1, 1).result;
        assert_eq!(
            result.kind(),
            M11CleanDocumentKind::Unknown(M11UnknownReason::UnsupportedList(reason)),
            "{source:?}: {:?}",
            result.leaves(),
        );
    }
}

#[test]
fn ordered_paragraph_interruption_and_digit_limit_follow_commonmark() {
    let noninterrupting = "paragraph\n2. stays text\n";
    let result = parse(noninterrupting, 1, 1).result;
    assert!(matches!(
        result.leaves(),
        [M11CleanLeaf::Paragraph {
            source: Range { start: 0, end: 24 },
            ..
        }]
    ));

    let interrupting = "paragraph\n1. becomes list\n";
    let result = parse(interrupting, 1, 1).result;
    assert!(matches!(
        result.leaves(),
        [
            M11CleanLeaf::Paragraph { .. },
            M11CleanLeaf::OrderedList {
                start: 1,
                delimiter: b'.',
                ..
            }
        ]
    ));

    let ten_digits = "1234567890. not a list\n";
    let result = parse(ten_digits, 1, 1).result;
    assert!(matches!(result.leaves(), [M11CleanLeaf::Paragraph { .. }]));
}

#[test]
fn unsupported_list_shapes_fail_closed_with_typed_parser_reasons() {
    let cases = [
        ("- [x] task\n", M11ListUnsupportedReason::Task),
        ("-\titem\n", M11ListUnsupportedReason::TabPadded),
        ("-     code\n", M11ListUnsupportedReason::ExcessivePadding),
        ("- - nested\n", M11ListUnsupportedReason::Nested),
        ("- # heading\n", M11ListUnsupportedReason::BlockChild),
        (
            "- first\ncontinuation\n",
            M11ListUnsupportedReason::LazyOrMultiline,
        ),
        (
            "- first\n  continuation\n",
            M11ListUnsupportedReason::LazyOrMultiline,
        ),
        ("- first\n\n- second\n", M11ListUnsupportedReason::Loose),
        ("- first\n  - nested\n", M11ListUnsupportedReason::Nested),
        (
            "- first\n-\n- next\n",
            M11ListUnsupportedReason::NonTerminalEmptyItem,
        ),
    ];
    for (source, reason) in cases {
        let result = parse(source, 1, 1).result;
        assert_eq!(
            result.kind(),
            M11CleanDocumentKind::Unknown(M11UnknownReason::UnsupportedList(reason)),
            "{source:?}: {:?}",
            result.leaves(),
        );
        assert_eq!(
            result.leaves(),
            &[ascii_unsupported(
                0..u32::try_from(source.len()).expect("fixture"),
                M11UnknownReason::UnsupportedList(reason),
            )],
            "{source:?}",
        );
    }
}

#[test]
fn list_boundary_blocks_remain_separate_exact_leaves() {
    let source = "- item\n# heading\n";
    let result = parse(source, 1, 1).result;
    assert!(matches!(
        result.leaves(),
        [
            M11CleanLeaf::BulletList { marker: b'-', .. },
            M11CleanLeaf::AtxHeading { level: 1, .. }
        ]
    ));

    let separated_markers = "- item\n\n+ other\n";
    let result = parse(separated_markers, 1, 1).result;
    assert!(matches!(
        result.leaves(),
        [
            M11CleanLeaf::BulletList { marker: b'-', .. },
            M11CleanLeaf::Blank { .. },
            M11CleanLeaf::BulletList { marker: b'+', .. }
        ]
    ));
}

#[test]
fn commonmark_107_through_118_preserve_top_level_indented_code_precedence() {
    let cases: [(&str, Vec<M11CleanLeaf>); 12] = [
        (
            "    a simple\n      indented code block\n",
            vec![ascii_indented_code(0..39, 2, 31, 1)],
        ),
        (
            "  - foo\n\n    bar\n",
            vec![ascii_unsupported(
                0..17,
                M11UnknownReason::UnsupportedList(M11ListUnsupportedReason::Loose),
            )],
        ),
        (
            "1.  foo\n\n    - bar\n",
            vec![ascii_unsupported(
                0..19,
                M11UnknownReason::UnsupportedList(M11ListUnsupportedReason::Loose),
            )],
        ),
        (
            "    <a/>\n    *hi*\n\n    - one\n",
            vec![ascii_indented_code(0..29, 4, 17, 1)],
        ),
        (
            "    chunk1\n\n    chunk2\n  \n \n \n    chunk3\n",
            vec![ascii_indented_code(0..41, 7, 25, 1)],
        ),
        (
            "    chunk1\n      \n      chunk2\n",
            vec![ascii_indented_code(0..31, 3, 19, 1)],
        ),
        (
            "Foo\n    bar\n\n",
            vec![ascii_paragraph(0..12, 0..12, 0), ascii_blank(12..13)],
        ),
        (
            "    foo\nbar\n",
            vec![
                ascii_indented_code(0..8, 1, 4, 1),
                ascii_paragraph(8..12, 8..12, 0),
            ],
        ),
        (
            "# Heading\n    foo\nHeading\n------\n    foo\n----\n",
            vec![
                ascii_atx_heading(0..10, 0..1, 2..9, None, 9..10, 1, 0),
                ascii_indented_code(10..18, 1, 4, 1),
                ascii_setext_heading(18..33, 18..25, 26..32, 32..33, 2, 0, 0),
                ascii_indented_code(33..41, 1, 4, 1),
                ascii_thematic_break(41..46, b'-', 4, 41..45, 45..46, 0),
            ],
        ),
        (
            "        foo\n    bar\n",
            vec![ascii_indented_code(0..20, 2, 12, 1)],
        ),
        (
            "\n    \n    foo\n    \n\n",
            vec![
                ascii_blank(0..6),
                ascii_indented_code(6..14, 1, 4, 1),
                ascii_blank(14..20),
            ],
        ),
        ("    foo  \n", vec![ascii_indented_code(0..10, 1, 6, 1)]),
    ];

    for (offset, (source, expected)) in cases.into_iter().enumerate() {
        let result = parse(source, 1, 1).result;
        assert_eq!(
            result.leaves(),
            expected,
            "CommonMark example {}: {source:?}",
            107 + offset,
        );
    }
}

#[test]
fn indented_code_geometry_is_exact_for_bom_tabs_unicode_nul_and_mixed_endings() {
    let source = "\u{feff}\tα\r\n    \tβ\r      γ\0";
    let result = parse(source, 1, 1).result;
    assert_eq!(result.kind(), M11CleanDocumentKind::Segmented);
    assert_eq!(
        result.leaves(),
        &[M11CleanLeaf::IndentedCode {
            source: 0..25,
            source_utf16: 0..20,
            line_count: 3,
            projected_utf8_length: 13,
            projected_utf16_length: 10,
            terminal_eol_bytes: 0,
            has_bof_bom: true,
        }]
    );
}

#[test]
fn indented_code_terminator_is_classified_once_with_normal_root_precedence() {
    let result = parse("    code\n# heading\n", 1, 1).result;
    assert_eq!(
        result.leaves(),
        &[
            ascii_indented_code(0..9, 1, 5, 1),
            ascii_atx_heading(9..19, 9..10, 11..18, None, 18..19, 1, 0),
        ]
    );

    let reference = parse("    code\n[x]: /target\n\nnext", 1, 1).result;
    assert_eq!(reference.definition_count(), 1);
    assert_eq!(
        reference.leaves(),
        &[
            ascii_indented_code(0..9, 1, 5, 1),
            ascii_definitions(9..22, 1),
            ascii_blank(22..23),
            ascii_paragraph(23..27, 23..27, 0),
        ]
    );
}

#[test]
fn gfm_table_delimiter_after_a_header_never_falls_through_to_paragraph() {
    let text = "left | right\n--- | ---\n";
    let result = parse(text, 1, 3).result;
    assert_eq!(
        result.kind(),
        M11CleanDocumentKind::Unknown(M11UnknownReason::UnsupportedOpener(
            M11UnsupportedOpener::TableCandidate,
        )),
    );
    assert_eq!(
        result.source_range(),
        0..u32::try_from(text.len()).expect("short fixture"),
    );
}

#[test]
fn leading_definitions_preserve_both_terminal_outcomes_and_source_cuts() {
    let definitions_only = "[Label]: /target \"title\"\n";
    let result = parse(definitions_only, 1, 2).result;
    assert_eq!(result.kind(), M11CleanDocumentKind::Empty);
    assert_eq!(
        result.source_range(),
        0..u32::try_from(definitions_only.len()).unwrap(),
    );
    assert_eq!(
        result.leaves(),
        &[ascii_definitions(0..definitions_only.len() as u32, 1)]
    );
    let definitions = result.definitions();
    assert_eq!(definitions.len(), 1);
    assert_eq!(definitions[0].normalized_label, "label");
    assert_eq!(definitions[0].label_source, 1..6);
    assert_eq!(definitions[0].destination_source, 9..16);

    let with_visible = "[x]: /target\nvisible paragraph\n";
    let result = parse(with_visible, 2, 3).result;
    assert_eq!(result.kind(), M11CleanDocumentKind::Paragraph);
    let definitions = result.definitions();
    assert_eq!(definitions.len(), 1);
    assert_eq!(
        result.visible_source().expect("paragraph visibility"),
        u32::try_from("[x]: /target\n".len()).unwrap()..u32::try_from(with_visible.len()).unwrap()
    );
    assert_eq!(
        result.leaves(),
        &[ascii_paragraph(
            0..with_visible.len() as u32,
            "[x]: /target\n".len() as u32..with_visible.len() as u32,
            1,
        )]
    );
}

#[test]
fn definitions_only_setext_branch_retains_the_paragraph_instead_of_retrying_thematic() {
    let text = "[x]: /target\n---\n";
    let result = parse(text, 1, 1).result;
    assert_eq!(result.kind(), M11CleanDocumentKind::Paragraph);
    let definitions = result.definitions();
    assert_eq!(definitions.len(), 1);
    assert_eq!(
        result.visible_source().expect("paragraph visibility"),
        u32::try_from("[x]: /target\n".len()).unwrap()..u32::try_from(text.len()).unwrap()
    );
}

#[test]
fn thematic_breaks_preserve_marker_envelopes_counts_and_every_line_ending() {
    let cases = [
        ("***\n", b'*', 3, 0, 0..3, 3..4),
        (" - - -  \r\n", b'-', 3, 1, 1..6, 8..10),
        ("  _\t_\t_ \r", b'_', 3, 2, 2..7, 8..9),
        ("   ****", b'*', 4, 3, 3..7, 7..7),
    ];
    for (text, marker, marker_count, opening_indent, marker_envelope, line_ending) in cases {
        let receipt = parse(text, 1, 1);
        assert_eq!(
            receipt.result.kind(),
            M11CleanDocumentKind::Segmented,
            "{text:?}"
        );
        assert_eq!(
            receipt.result.leaves(),
            &[ascii_thematic_break(
                0..text.len() as u32,
                marker,
                marker_count,
                marker_envelope,
                line_ending,
                opening_indent,
            )],
            "{text:?}",
        );
        assert_eq!(receipt.source_reads, text.len(), "{text:?}");
    }

    let text = "\u{feff}***\n";
    let result = parse(text, 1, 1).result;
    assert_eq!(
        result.leaves(),
        &[M11CleanLeaf::ThematicBreak {
            source: 0..7,
            source_utf16: 0..5,
            marker: b'*',
            marker_count: 3,
            marker_envelope: 3..6,
            line_ending: 6..7,
            opening_indent: 0,
            has_bof_bom: true,
        }]
    );
}

#[test]
fn commonmark_thematic_break_examples_keep_top_level_precedence() {
    let rules = [
        "***\n",
        "---\n",
        "___\n",
        " ***\n",
        "  ***\n",
        "   ***\n",
        "_____________________________________\n",
        " - - -\n",
        " **  * ** * ** * **\n",
        "-     -      -      -\n",
        "- - - -    \n",
    ];
    for text in rules {
        let result = parse(text, 1, 1).result;
        assert!(
            matches!(result.leaves(), [M11CleanLeaf::ThematicBreak { .. }]),
            "CommonMark thematic-break line {text:?}: {result:?}",
        );
    }

    for text in [
        "+++\n",
        "===\n",
        "_ _ _ _ a\n",
        "a------\n",
        "---a---\n",
        " *-*\n",
    ] {
        let result = parse(text, 1, 1).result;
        assert_eq!(result.kind(), M11CleanDocumentKind::Paragraph, "{text:?}");
    }
    assert_eq!(
        parse("--\n**\n__\n", 1, 1).result.kind(),
        M11CleanDocumentKind::Unknown(M11UnknownReason::UnsupportedOpener(
            M11UnsupportedOpener::TableCandidate,
        )),
        "the current GFM table detector must fail closed, not promote a near miss to thematic",
    );

    let indented = parse("    ***\n", 1, 1).result;
    assert_eq!(indented.leaves(), &[ascii_indented_code(0..8, 1, 4, 1)],);
    assert_eq!(
        parse("Foo\n    ***\n", 1, 1).result.kind(),
        M11CleanDocumentKind::Paragraph,
        "indented code cannot interrupt a Paragraph",
    );

    let interrupted = parse("Foo\n***\nbar\n", 1, 1).result;
    assert!(matches!(
        interrupted.leaves(),
        [
            M11CleanLeaf::Paragraph { .. },
            M11CleanLeaf::ThematicBreak { .. },
            M11CleanLeaf::Paragraph { .. },
        ]
    ));

    let setext = parse("Foo\n---\nbar\n", 1, 1).result;
    assert!(matches!(
        setext.leaves(),
        [
            M11CleanLeaf::SetextHeading { level: 2, .. },
            M11CleanLeaf::Paragraph { .. },
        ]
    ));

    let root_break = parse("- foo\n***\n- bar\n", 1, 1).result;
    assert!(matches!(
        root_break.leaves(),
        [
            M11CleanLeaf::BulletList { marker: b'-', .. },
            M11CleanLeaf::ThematicBreak { marker: b'*', .. },
            M11CleanLeaf::BulletList { marker: b'-', .. },
        ]
    ));

    let list_item_text = parse("* Foo\n* * *\n* Bar\n", 1, 1).result;
    assert_eq!(
        list_item_text.kind(),
        M11CleanDocumentKind::Unknown(M11UnknownReason::UnsupportedList(
            M11ListUnsupportedReason::Nested,
        )),
    );

    let child_break = parse("- Foo\n- * * *\n", 1, 1).result;
    assert_eq!(
        child_break.kind(),
        M11CleanDocumentKind::Unknown(M11UnknownReason::UnsupportedList(
            M11ListUnsupportedReason::BlockChild,
        )),
    );
}

#[test]
fn thematic_break_after_leading_definitions_keeps_both_authorities_exact() {
    let text = "[x]: /target\n***\r\n";
    let result = parse(text, 1, 1).result;
    assert_eq!(result.definitions().len(), 1);
    assert_eq!(
        result.leaves(),
        &[
            ascii_definitions(0..13, 1),
            ascii_thematic_break(13..18, b'*', 3, 13..16, 16..18, 0),
        ],
    );
}

#[test]
fn setext_headings_preserve_level_indent_trailing_space_and_every_line_ending() {
    let cases = [
        ("title\n===\n", 1_u8, 0_u8, 6..9, 9..10),
        ("title\r\n ---  \r\n", 2, 1, 8..11, 13..15),
        ("title\r  == \r", 1, 2, 8..10, 11..12),
        ("title\n---", 2, 0, 6..9, 9..9),
    ];
    for (text, level, opening_indent, marker, line_ending) in cases {
        let result = parse(text, 1, 1).result;
        assert_eq!(result.kind(), M11CleanDocumentKind::Segmented, "{text:?}");
        assert_eq!(
            result.leaves(),
            &[ascii_setext_heading(
                0..text.len() as u32,
                0..5,
                marker,
                line_ending,
                level,
                opening_indent,
                0,
            )],
            "{text:?}"
        );
    }
}

#[test]
fn multiline_unicode_setext_and_reference_visible_content_keep_exact_geometry() {
    let text = "α😀\nsecond β\r\n  --- \r";
    let underline_start = text.rfind("  ---").expect("underline");
    let marker_start = underline_start + 2;
    let marker_end = marker_start + 3;
    let ending_start = text.len() - 1;
    let result = parse(text, 2, 1).result;
    assert_eq!(
        result.leaves(),
        &[M11CleanLeaf::SetextHeading {
            source: 0..text.len() as u32,
            source_utf16: 0..text.encode_utf16().count() as u32,
            inline_source: 0..underline_start as u32 - 2,
            underline_marker: marker_start as u32..marker_end as u32,
            underline_line_ending: ending_start as u32..text.len() as u32,
            level: 2,
            opening_indent: 2,
            reference_definition_count: 0,
        }]
    );

    let text = "[x]: /target\nvisible **β**\n---\n";
    let visible_start = text.find("visible").expect("visible");
    let underline_start = text.rfind("---").expect("underline");
    let result = parse(text, 1, 2).result;
    assert_eq!(result.definitions().len(), 1);
    assert_eq!(
        result.leaves(),
        &[M11CleanLeaf::SetextHeading {
            source: 0..text.len() as u32,
            source_utf16: 0..text.encode_utf16().count() as u32,
            inline_source: visible_start as u32..underline_start as u32 - 1,
            underline_marker: underline_start as u32..underline_start as u32 + 3,
            underline_line_ending: text.len() as u32 - 1..text.len() as u32,
            level: 2,
            opening_indent: 0,
            reference_definition_count: 1,
        }]
    );
}

#[test]
fn setext_near_misses_remain_paragraph_or_follow_later_opener_precedence() {
    for text in ["title\n    ===\n", "title\n\t===\n", "title\n=== nope\n"] {
        let result = parse(text, 1, 1).result;
        assert_eq!(result.kind(), M11CleanDocumentKind::Paragraph, "{text:?}");
        assert!(matches!(result.leaves(), [M11CleanLeaf::Paragraph { .. }]));
    }

    let thematic = parse("title\n- - -\n", 1, 1).result;
    assert!(matches!(
        thematic.leaves(),
        [
            M11CleanLeaf::Paragraph { .. },
            M11CleanLeaf::ThematicBreak { marker: b'-', .. },
        ]
    ));
}

#[test]
fn safe_giant_paragraph_is_resumable_and_retention_stays_bounded() {
    let text = "a".repeat(M11_SEGMENTED_LINE_PREFIX_BYTES * 30);
    let receipt = parse(&text, 7, SOURCE_CURSOR_WINDOW_BYTES);
    assert_eq!(receipt.result.kind(), M11CleanDocumentKind::Paragraph);
    assert_eq!(receipt.source_reads, text.len());
    assert!(receipt.max_retained <= M11_SEGMENTED_LINE_PREFIX_BYTES);
    assert!(receipt.max_work <= 7);
}

#[test]
fn giant_special_prefix_is_decided_by_grammar_not_size() {
    let mut text = "#".to_owned();
    text.push_str(&"x".repeat(10 * 1024 * 1024));
    let receipt = parse(&text, 17, SOURCE_CURSOR_WINDOW_BYTES);
    assert_eq!(receipt.result.kind(), M11CleanDocumentKind::Paragraph);
    assert_eq!(receipt.source_reads, text.len());
    assert!(receipt.max_retained <= M11_SEGMENTED_LINE_PREFIX_BYTES);
    assert!(receipt.max_work <= 17);
}

#[test]
fn giant_atx_heading_is_authoritative_resumable_and_source_bounded() {
    let mut text = String::with_capacity(10 * 1024 * 1024 + 16);
    text.push_str("# ");
    let body_bytes = 10 * 1024 * 1024;
    text.push_str(&"a".repeat(body_bytes));
    text.push_str(" ###\r\n");
    let receipt = parse(&text, 7, SOURCE_CURSOR_WINDOW_BYTES);
    let body_end = 2 + body_bytes;
    assert_eq!(receipt.result.kind(), M11CleanDocumentKind::Segmented);
    assert_eq!(
        receipt.result.leaves(),
        &[ascii_atx_heading(
            0..text.len() as u32,
            0..1,
            2..body_end as u32,
            Some((body_end + 1) as u32..(body_end + 4) as u32),
            (text.len() - 2) as u32..text.len() as u32,
            1,
            0,
        )]
    );
    assert_eq!(receipt.source_reads, text.len());
    assert!(receipt.max_retained <= M11_SEGMENTED_LINE_PREFIX_BYTES);
    assert!(receipt.max_work <= 7);
}

#[test]
fn atx_heading_partitions_mixed_blocks_and_preserves_dual_geometry() {
    let text = "before\n\n  ###  **β😀** ###  \r\nafter";
    let result = parse(text, 2, 1).result;
    assert_eq!(result.kind(), M11CleanDocumentKind::Segmented);
    assert_eq!(
        result.leaves(),
        &[
            ascii_paragraph(0..7, 0..7, 0),
            ascii_blank(7..8),
            M11CleanLeaf::AtxHeading {
                source: 8..33,
                source_utf16: 8..30,
                opening_marker: 10..13,
                inline_source: 15..25,
                closing_marker: Some(26..29),
                line_ending: 31..33,
                level: 3,
                opening_indent: 2,
                has_bof_bom: false,
            },
            M11CleanLeaf::Paragraph {
                source: 33..38,
                source_utf16: 30..35,
                inline_source: 33..38,
                reference_definition_count: 0,
            },
        ]
    );
}

#[test]
fn bof_bom_atx_heading_carries_explicit_prefix_authority() {
    let text = "\u{feff} # x ###\r\n";
    let result = parse(text, 1, 1).result;
    assert_eq!(result.kind(), M11CleanDocumentKind::Segmented);
    assert_eq!(
        result.leaves(),
        &[M11CleanLeaf::AtxHeading {
            source: 0..13,
            source_utf16: 0..11,
            opening_marker: 4..5,
            inline_source: 6..7,
            closing_marker: Some(8..11),
            line_ending: 11..13,
            level: 1,
            opening_indent: 1,
            has_bof_bom: true,
        }]
    );
}

#[test]
fn giant_reference_values_are_source_ranges_with_bounded_retention() {
    let payload = 10 * 1024 * 1024;
    let destination = format!("[x]: /{}\n", "u".repeat(payload));
    let receipt = parse(&destination, 31, SOURCE_CURSOR_WINDOW_BYTES);
    assert_eq!(receipt.result.kind(), M11CleanDocumentKind::Empty);
    let definitions = receipt.result.definitions();
    assert_eq!(definitions.len(), 1);
    assert_eq!(
        definitions[0].destination_source.end - definitions[0].destination_source.start,
        u32::try_from(payload + 1).unwrap()
    );
    assert!(receipt.max_retained <= M11_SEGMENTED_LINE_PREFIX_BYTES + 1001);
    assert!(receipt.max_work <= 31);

    let title = format!("[x]: /u \"{}\"\n", "t".repeat(payload));
    let receipt = parse(&title, 31, SOURCE_CURSOR_WINDOW_BYTES);
    assert_eq!(receipt.result.kind(), M11CleanDocumentKind::Empty);
    let definitions = receipt.result.definitions();
    let title_source = definitions[0].title_source.as_ref().expect("title range");
    assert_eq!(
        title_source.end - title_source.start,
        u32::try_from(payload + 2).unwrap()
    );
    assert!(receipt.max_retained <= M11_SEGMENTED_LINE_PREFIX_BYTES + 1001);
    assert!(receipt.max_work <= 31);
}

#[test]
fn giant_line_can_be_cancelled_without_publishing_partial_grammar() {
    let mut text = "#".to_owned();
    text.push_str(&"x".repeat(10 * 1024 * 1024));
    let store = SourceStore::new(&text).expect("source");
    let mut scanner = SnapshotLineScanner::new(store.snapshot()).expect("scanner");
    let line = loop {
        match scanner.poll(17).expect("line discovery") {
            SnapshotLinePoll::Pending(next) => scanner = next,
            SnapshotLinePoll::Line(line) => break line,
            SnapshotLinePoll::Complete => panic!("source has a first line"),
        }
    };
    let facts = line.facts();
    let mut source = line.into_source().expect("line source");
    let mut controller = M11CleanBlockController::new();
    let mut admission =
        <M11CleanBlockController as M11ExactController<SnapshotLineSource>>::begin_source_line(
            &mut controller,
            facts.identity(),
        )
        .expect("begin line");

    assert_eq!(source.replenish_access_budget(17).expect("grant"), 17);
    let receipt =
        <M11CleanBlockController as M11ExactController<SnapshotLineSource>>::poll_source_line(
            &mut controller,
            &mut admission,
            &mut source,
            17,
        )
        .expect("poll line");
    assert_ne!(receipt.status, M11SourceLinePollStatus::Matched);
    assert_eq!(receipt.source_first_reads, 17);
    assert_eq!(receipt.lexical_work_units, 17);
    assert!(receipt.retained_source_bytes <= M11_SEGMENTED_LINE_PREFIX_BYTES);

    <M11CleanBlockController as M11ExactController<SnapshotLineSource>>::cancel_source_line(
        &mut controller,
        admission,
    )
    .expect("cancel grammar admission");
    let (cancellation, scanner) = source.cancel();
    assert_eq!(cancellation.identity, facts.identity());
    assert_eq!(cancellation.bytes_read, 17);
    assert_eq!(cancellation.unused_access_budget, 0);
    drop(scanner);

    assert_eq!(
        controller.finish(),
        Err(M11CleanControllerFault::SourceIncomplete {
            expected: text.len(),
            actual: 0,
        })
    );
}

#[test]
fn blank_boundaries_partition_exact_paragraph_and_gap_leaves() {
    let text = "first\n\nsecond";
    let result = parse(text, 2, 2).result;
    assert_eq!(result.kind(), M11CleanDocumentKind::Segmented);
    assert_eq!(result.source_range(), 0..u32::try_from(text.len()).unwrap(),);
    assert_eq!(
        result.leaves(),
        &[
            ascii_paragraph(0..6, 0..6, 0),
            ascii_blank(6..7),
            ascii_paragraph(7..13, 7..13, 0),
        ]
    );
    assert!(!result.has_unknown_coverage());
}

#[test]
fn blank_leaf_ranges_preserve_leading_trailing_and_crlf_source() {
    let cases = [
        (
            "\r\none\r\n\r\ntwo\r\n\r\n",
            vec![
                ascii_blank(0..2),
                ascii_paragraph(2..7, 2..7, 0),
                ascii_blank(7..9),
                ascii_paragraph(9..14, 9..14, 0),
                ascii_blank(14..16),
            ],
        ),
        (
            "\n\nplain",
            vec![ascii_blank(0..2), ascii_paragraph(2..7, 2..7, 0)],
        ),
        ("\r\n\n", vec![ascii_blank(0..3)]),
    ];
    for (text, expected) in cases {
        let baseline = parse(text, 1, 1).result;
        assert_eq!(baseline.leaves(), expected, "{text:?}");
        for fuel in [2, 7, 31] {
            assert_eq!(
                parse(text, fuel, 3).result.leaves(),
                expected,
                "{text:?}, fuel={fuel}"
            );
        }
    }
}

#[test]
fn leaf_utf16_ranges_partition_unicode_source_without_rescanning_inline_content() {
    let text = "α😀\n\nβ";
    let result = parse(text, 1, 2).result;
    assert_eq!(
        result.leaves(),
        &[
            M11CleanLeaf::Paragraph {
                source: 0..7,
                source_utf16: 0..4,
                inline_source: 0..7,
                reference_definition_count: 0,
            },
            M11CleanLeaf::Blank {
                source: 7..8,
                source_utf16: 4..5,
            },
            M11CleanLeaf::Paragraph {
                source: 8..10,
                source_utf16: 5..6,
                inline_source: 8..10,
                reference_definition_count: 0,
            },
        ]
    );
}

#[test]
fn inserting_and_deleting_a_blank_line_splits_and_merges_paragraphs() {
    let merged = parse("one\ntwo", 1, 2).result;
    assert_eq!(merged.leaves(), &[ascii_paragraph(0..7, 0..7, 0)]);

    let split = parse("one\n\ntwo", 1, 2).result;
    assert_eq!(
        split.leaves(),
        &[
            ascii_paragraph(0..4, 0..4, 0),
            ascii_blank(4..5),
            ascii_paragraph(5..8, 5..8, 0),
        ]
    );
}

#[test]
fn leaf_accumulation_is_not_capped_at_the_old_flat_role_limit() {
    let mut text = String::new();
    for ordinal in 0..130 {
        if ordinal != 0 {
            text.push('\n');
        }
        text.push_str(&format!("paragraph-{ordinal}\n"));
    }
    let result = parse(&text, 7, 5).result;
    assert_eq!(result.kind(), M11CleanDocumentKind::Segmented);
    assert_eq!(result.leaves().len(), 259);
    let mut expected_start = 0;
    let mut expected_start_utf16 = 0;
    for leaf in result.leaves() {
        let source = leaf.source_range();
        let source_utf16 = leaf.source_utf16_range();
        assert_eq!(source.start, expected_start);
        assert_eq!(source_utf16.start, expected_start_utf16);
        assert!(source.start < source.end);
        assert!(source_utf16.start < source_utf16.end);
        expected_start = source.end;
        expected_start_utf16 = source_utf16.end;
    }
    assert_eq!(expected_start, text.len() as u32);
    assert_eq!(expected_start_utf16, text.encode_utf16().count() as u32);
}

#[test]
fn unsupported_winners_roll_back_only_the_open_paragraph_suffix() {
    let whole = parse("plain\n---\n", 1, 1).result;
    assert_eq!(
        whole.leaves(),
        &[ascii_setext_heading(0..10, 0..5, 6..9, 9..10, 2, 0, 0)]
    );
    assert_eq!(whole.kind(), M11CleanDocumentKind::Segmented);

    let prefix = "safe\n\nnext\n| --- |\n";
    let result = parse(prefix, 1, 2).result;
    assert_eq!(
        result.leaves(),
        &[
            ascii_paragraph(0..5, 0..5, 0),
            ascii_blank(5..6),
            ascii_unsupported(
                6..prefix.len() as u32,
                M11UnknownReason::UnsupportedOpener(M11UnsupportedOpener::TableCandidate),
            ),
        ]
    );
    assert!(result.has_unknown_coverage());
}

#[test]
fn finish_rejects_a_prefix_even_when_no_line_admission_is_active() {
    let text = "first\nsecond";
    let store = SourceStore::new(text).expect("source");
    let mut scanner = SnapshotLineScanner::new(store.snapshot()).expect("scanner");
    let line = loop {
        match scanner.poll(2).expect("line discovery") {
            SnapshotLinePoll::Pending(next) => scanner = next,
            SnapshotLinePoll::Line(line) => break line,
            SnapshotLinePoll::Complete => panic!("source has a first line"),
        }
    };
    let facts = line.facts();
    let mut source = line.into_source().expect("line source");
    let mut controller = M11CleanBlockController::new();
    let mut admission =
        <M11CleanBlockController as M11ExactController<SnapshotLineSource>>::begin_source_line(
            &mut controller,
            facts.identity(),
        )
        .expect("begin line");
    loop {
        if source.access_budget() == 0 && source.position() < source.len() {
            source.replenish_access_budget(2).expect("source grant");
        }
        let receipt =
            <M11CleanBlockController as M11ExactController<SnapshotLineSource>>::poll_source_line(
                &mut controller,
                &mut admission,
                &mut source,
                2,
            )
            .expect("poll line");
        if receipt.status == M11SourceLinePollStatus::Matched {
            break;
        }
    }
    <M11CleanBlockController as M11ExactController<SnapshotLineSource>>::commit_source_line(
        &mut controller,
        admission,
        facts,
    )
    .expect("commit first line");

    assert_eq!(
        controller.finish(),
        Err(M11CleanControllerFault::SourceIncomplete {
            expected: text.len(),
            actual: "first\n".len(),
        })
    );
}

#[test]
fn block_quote_depth_one_maps_marked_and_lazy_paragraph_lines_exactly() {
    let text = "   > alpha\n> beta\nlazy\n";
    let result = parse(text, 1, 1).result;
    let [M11CleanLeaf::BlockQuote {
        source,
        source_utf16,
        lines,
        child_paragraph,
        disposition,
    }] = result.leaves()
    else {
        panic!("expected one block quote: {:?}", result.leaves());
    };
    assert_eq!(source, &(0..23));
    assert_eq!(source_utf16, &(0..23));
    assert_eq!(
        lines.as_ref(),
        &[
            M11BlockQuoteLineMapping {
                source: 0..11,
                source_utf16: 0..11,
                opening_marker: Some(3..4),
                hidden_prefix: Some(0..5),
                content_source: 5..10,
                content_source_utf16: 5..10,
                line_ending: 10..11,
                line_ending_utf16: 10..11,
                residual_tab_columns: 0,
                kind: M11BlockQuoteLineKind::MarkedParagraph,
            },
            M11BlockQuoteLineMapping {
                source: 11..18,
                source_utf16: 11..18,
                opening_marker: Some(11..12),
                hidden_prefix: Some(11..13),
                content_source: 13..17,
                content_source_utf16: 13..17,
                line_ending: 17..18,
                line_ending_utf16: 17..18,
                residual_tab_columns: 0,
                kind: M11BlockQuoteLineKind::MarkedParagraph,
            },
            M11BlockQuoteLineMapping {
                source: 18..23,
                source_utf16: 18..23,
                opening_marker: None,
                hidden_prefix: None,
                content_source: 18..22,
                content_source_utf16: 18..22,
                line_ending: 22..23,
                line_ending_utf16: 22..23,
                residual_tab_columns: 0,
                kind: M11BlockQuoteLineKind::LazyParagraphContinuation,
            },
        ]
    );
    assert_eq!(
        child_paragraph,
        &Some(M11BlockQuoteParagraphMapping {
            line_indices: 0..3,
            projected_utf8_length: 16,
            projected_utf16_length: 16,
        })
    );
    assert_eq!(disposition, &M11BlockQuoteDisposition::ExactSingleParagraph);
    assert!(!result.has_unknown_coverage());
}

#[test]
fn commonmark_230_and_231_keep_quote_indentation_precedence_fail_closed() {
    let quote = parse("   > # Foo\n   > bar\n > baz\n", 1, 1).result;
    let [M11CleanLeaf::BlockQuote {
        source,
        disposition,
        lines,
        child_paragraph,
        ..
    }] = quote.leaves()
    else {
        panic!("expected the depth-1 quote envelope");
    };
    assert_eq!(source, &(0..27));
    assert_eq!(lines.len(), 3);
    assert_eq!(
        disposition,
        &M11BlockQuoteDisposition::Unsupported(M11BlockQuoteUnsupportedReason::AtxHeading)
    );
    assert!(child_paragraph.is_none());
    assert!(quote.has_unknown_coverage());

    let indented = parse("    > # Foo\n    > bar\n    > baz\n", 1, 1).result;
    assert!(matches!(
        indented.leaves(),
        [M11CleanLeaf::IndentedCode { .. }]
    ));
}

#[test]
fn commonmark_232_234_and_237_separate_lazy_text_from_real_block_openers() {
    let lazy = parse("> bar\nbaz\n> qux\n", 1, 1).result;
    let [M11CleanLeaf::BlockQuote {
        disposition,
        child_paragraph: Some(child),
        lines,
        ..
    }] = lazy.leaves()
    else {
        panic!("expected a supported lazy quote");
    };
    assert_eq!(disposition, &M11BlockQuoteDisposition::ExactSingleParagraph);
    assert_eq!(child.line_indices, 0..3);
    assert_eq!(
        lines[1].kind,
        M11BlockQuoteLineKind::LazyParagraphContinuation
    );

    let thematic = parse("> foo\n---\n", 1, 1).result;
    assert!(matches!(
        thematic.leaves(),
        [
            M11CleanLeaf::BlockQuote {
                disposition: M11BlockQuoteDisposition::ExactSingleParagraph,
                ..
            },
            M11CleanLeaf::ThematicBreak { .. }
        ]
    ));
    assert_eq!(thematic.leaves()[0].source_range(), 0..6);
    assert_eq!(thematic.leaves()[1].source_range(), 6..10);

    let fenced = parse("> ```\nfoo\n```\n", 1, 1).result;
    assert!(matches!(
        fenced.leaves(),
        [
            M11CleanLeaf::BlockQuote {
                source,
                disposition: M11BlockQuoteDisposition::Unsupported(
                    M11BlockQuoteUnsupportedReason::FencedCode
                ),
                ..
            },
            M11CleanLeaf::Paragraph { .. },
            M11CleanLeaf::FencedCode { .. },
        ] if source == &(0..6)
    ));
}

#[test]
fn commonmark_239_242_and_244_make_blank_container_boundaries_explicit() {
    let empty = parse(">\n", 1, 1).result;
    assert!(matches!(
        empty.leaves(),
        [M11CleanLeaf::BlockQuote {
            disposition: M11BlockQuoteDisposition::Unsupported(
                M11BlockQuoteUnsupportedReason::MarkerOnlyOrBlank
            ),
            child_paragraph: None,
            ..
        }]
    ));

    let separated = parse("> foo\n\n> bar\n", 1, 1).result;
    assert!(matches!(
        separated.leaves(),
        [
            M11CleanLeaf::BlockQuote {
                disposition: M11BlockQuoteDisposition::ExactSingleParagraph,
                ..
            },
            M11CleanLeaf::Blank { .. },
            M11CleanLeaf::BlockQuote {
                disposition: M11BlockQuoteDisposition::ExactSingleParagraph,
                ..
            },
        ]
    ));
    assert_eq!(separated.leaves()[0].source_range(), 0..6);
    assert_eq!(separated.leaves()[1].source_range(), 6..7);
    assert_eq!(separated.leaves()[2].source_range(), 7..13);

    let multiple = parse("> foo\n>\n> bar\n", 1, 1).result;
    assert!(matches!(
        multiple.leaves(),
        [M11CleanLeaf::BlockQuote {
            source,
            disposition: M11BlockQuoteDisposition::Unsupported(
                M11BlockQuoteUnsupportedReason::MultipleParagraphChildren
            ),
            child_paragraph: None,
            ..
        }] if source == &(0..14)
    ));
}

#[test]
fn commonmark_250_and_251_withhold_nested_quote_semantics_without_eating_boundaries() {
    let lazy_nested = parse("> > > foo\nbar\n", 1, 1).result;
    assert!(matches!(
        lazy_nested.leaves(),
        [
            M11CleanLeaf::BlockQuote {
                source,
                disposition: M11BlockQuoteDisposition::Unsupported(
                    M11BlockQuoteUnsupportedReason::NestedBlockQuote
                ),
                ..
            },
            M11CleanLeaf::Paragraph { .. },
        ] if source == &(0..10)
    ));
    assert_eq!(lazy_nested.leaves()[1].source_range(), 10..14);

    let marked_nested = parse(">>> foo\n> bar\n>>baz\n", 1, 1).result;
    assert!(matches!(
        marked_nested.leaves(),
        [M11CleanLeaf::BlockQuote {
            source,
            lines,
            disposition: M11BlockQuoteDisposition::Unsupported(
                M11BlockQuoteUnsupportedReason::NestedBlockQuote
            ),
            ..
        }] if source == &(0..20) && lines.len() == 3
    ));
}

#[test]
fn commonmark_235_and_252_fail_closed_for_list_code_and_reference_children() {
    let list = parse("> - foo\n- bar\n", 1, 1).result;
    assert!(matches!(
        list.leaves(),
        [
            M11CleanLeaf::BlockQuote {
                source,
                disposition: M11BlockQuoteDisposition::Unsupported(
                    M11BlockQuoteUnsupportedReason::List
                ),
                ..
            },
            M11CleanLeaf::BulletList { marker: b'-', .. },
        ] if source == &(0..8)
    ));

    let code = parse(">     code\n\n>    not code\n", 1, 1).result;
    assert!(matches!(
        code.leaves(),
        [
            M11CleanLeaf::BlockQuote {
                source,
                disposition: M11BlockQuoteDisposition::Unsupported(
                    M11BlockQuoteUnsupportedReason::IndentedCode
                ),
                ..
            },
            M11CleanLeaf::Blank { .. },
            M11CleanLeaf::BlockQuote {
                disposition: M11BlockQuoteDisposition::ExactSingleParagraph,
                ..
            },
        ] if source == &(0..11)
    ));

    let reference = parse("> [foo]: /url\n", 1, 1).result;
    assert!(matches!(
        reference.leaves(),
        [M11CleanLeaf::BlockQuote {
            disposition: M11BlockQuoteDisposition::Unsupported(
                M11BlockQuoteUnsupportedReason::PotentialReferenceDefinition
            ),
            child_paragraph: None,
            ..
        }]
    ));
}

#[test]
fn block_quote_tabs_classify_from_the_container_column() {
    let cases = [
        ("> \tfoo\n", M11BlockQuoteDisposition::ExactSingleParagraph),
        (">  \tfoo\n", M11BlockQuoteDisposition::ExactSingleParagraph),
        (
            ">   \tfoo\n",
            M11BlockQuoteDisposition::Unsupported(M11BlockQuoteUnsupportedReason::IndentedCode),
        ),
        (" > \tfoo\n", M11BlockQuoteDisposition::ExactSingleParagraph),
        (
            "  > \tfoo\n",
            M11BlockQuoteDisposition::Unsupported(M11BlockQuoteUnsupportedReason::IndentedCode),
        ),
        (
            "   > \tfoo\n",
            M11BlockQuoteDisposition::ExactSingleParagraph,
        ),
        (
            ">\tfoo\n",
            M11BlockQuoteDisposition::Unsupported(M11BlockQuoteUnsupportedReason::PartialTabMarker),
        ),
        ("  >\tfoo\n", M11BlockQuoteDisposition::ExactSingleParagraph),
    ];

    for (source, expected) in cases {
        let result = parse(source, 1, 1).result;
        let [M11CleanLeaf::BlockQuote { disposition, .. }] = result.leaves() else {
            panic!(
                "expected one block quote for {source:?}: {:?}",
                result.leaves()
            );
        };
        assert_eq!(*disposition, expected, "{source:?}");
    }
}

#[test]
fn unsupported_reference_candidates_retain_commonmark_lazy_quote_ownership() {
    for (text, expected_html) in [
        (
            "> [foo]: /url\nbar\n",
            "<blockquote>\n<p>bar</p>\n</blockquote>\n",
        ),
        (
            "> [not a definition]\nbar\n",
            "<blockquote>\n<p>[not a definition]\nbar</p>\n</blockquote>\n",
        ),
    ] {
        assert_eq!(
            markdown_to_html(text, &ComrakOptions::default()),
            expected_html,
            "Comrak ownership oracle changed for {text:?}",
        );
        let result = parse(text, 1, 1).result;
        let [M11CleanLeaf::BlockQuote {
            source,
            lines,
            child_paragraph,
            disposition,
            ..
        }] = result.leaves()
        else {
            panic!(
                "expected one source-owning quote envelope: {:?}",
                result.leaves()
            );
        };
        assert_eq!(
            source,
            &(0..u32::try_from(text.len()).expect("short fixture"))
        );
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].kind, M11BlockQuoteLineKind::MarkedUnsupported);
        assert_eq!(
            lines[1].kind,
            M11BlockQuoteLineKind::LazyParagraphContinuation,
        );
        assert!(child_paragraph.is_none());
        assert_eq!(
            disposition,
            &M11BlockQuoteDisposition::Unsupported(
                M11BlockQuoteUnsupportedReason::PotentialReferenceDefinition,
            ),
        );
    }
}

#[test]
fn unsupported_quote_can_later_open_a_paragraph_that_owns_lazy_lines() {
    let text = "> # heading\n> paragraph\nlazy\n";
    assert_eq!(
        markdown_to_html(text, &ComrakOptions::default()),
        "<blockquote>\n<h1>heading</h1>\n<p>paragraph\nlazy</p>\n</blockquote>\n",
    );

    let result = parse(text, 1, 1).result;
    let [M11CleanLeaf::BlockQuote {
        source,
        lines,
        child_paragraph,
        disposition,
        ..
    }] = result.leaves()
    else {
        panic!(
            "expected one source-owning quote envelope: {:?}",
            result.leaves()
        );
    };
    assert_eq!(
        source,
        &(0..u32::try_from(text.len()).expect("short fixture"))
    );
    assert_eq!(lines.len(), 3);
    assert_eq!(lines[0].kind, M11BlockQuoteLineKind::MarkedUnsupported);
    assert_eq!(lines[1].kind, M11BlockQuoteLineKind::MarkedUnsupported);
    assert_eq!(
        lines[2].kind,
        M11BlockQuoteLineKind::LazyParagraphContinuation,
    );
    assert!(child_paragraph.is_none());
    assert_eq!(
        disposition,
        &M11BlockQuoteDisposition::Unsupported(M11BlockQuoteUnsupportedReason::AtxHeading),
    );
}
