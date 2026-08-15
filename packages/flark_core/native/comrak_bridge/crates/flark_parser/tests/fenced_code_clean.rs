use flark_engine::SourceStore;
use flark_parser::{
    M11CleanBlockController, M11CleanDocumentKind, M11CleanDocumentResult, M11CleanLeaf,
    M11ExactController, M11SourceLinePollStatus, M11SourceLineSource, SnapshotLinePoll,
    SnapshotLineScanner, SnapshotLineSource,
};

fn parse(text: &str) -> M11CleanDocumentResult {
    let store = SourceStore::new(text).expect("source");
    let mut scanner = SnapshotLineScanner::new(store.snapshot()).expect("scanner");
    let mut controller = M11CleanBlockController::new_for_source(store.version());

    loop {
        let line = loop {
            match scanner.poll(3).expect("line discovery") {
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
                source.replenish_access_budget(2).expect("source grant");
            }
            let receipt = <M11CleanBlockController as M11ExactController<
                SnapshotLineSource,
            >>::poll_source_line(&mut controller, &mut admission, &mut source, 3)
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
        .expect("commit line");
        scanner = source.finish().expect("line consumed");
    }

    controller
        .finish()
        .unwrap_or_else(|error| panic!("finish document {text:?}: {error:?}"))
}

fn sole_fence(result: &M11CleanDocumentResult) -> &M11CleanLeaf {
    assert_eq!(result.kind(), M11CleanDocumentKind::Segmented);
    assert_eq!(result.definition_count(), 0);
    assert!(result.visible_source().is_none());
    let [leaf @ M11CleanLeaf::FencedCode { .. }] = result.leaves() else {
        panic!("expected sole fenced-code leaf: {:?}", result.leaves());
    };
    leaf
}

#[test]
fn closed_backtick_fence_preserves_exact_crlf_unicode_ranges() {
    let text = "  ``` rust\r\nα😀\r\n  ```\r\n";
    assert_eq!(
        sole_fence(&parse(text)),
        &M11CleanLeaf::FencedCode {
            source: 0..27,
            source_utf16: 0..24,
            opening_marker: 2..5,
            raw_info_source: 5..10,
            body_source: 12..20,
            closing_marker: Some(22..25),
            marker: b'`',
            opening_indent: 2,
        }
    );
}

#[test]
fn tilde_and_backtick_fences_are_authoritative_when_unclosed() {
    assert_eq!(
        sole_fence(&parse("~~~lang\nx")),
        &M11CleanLeaf::FencedCode {
            source: 0..9,
            source_utf16: 0..9,
            opening_marker: 0..3,
            raw_info_source: 3..7,
            body_source: 8..9,
            closing_marker: None,
            marker: b'~',
            opening_indent: 0,
        }
    );
    assert_eq!(
        sole_fence(&parse("```")),
        &M11CleanLeaf::FencedCode {
            source: 0..3,
            source_utf16: 0..3,
            opening_marker: 0..3,
            raw_info_source: 3..3,
            body_source: 3..3,
            closing_marker: None,
            marker: b'`',
            opening_indent: 0,
        }
    );
}

#[test]
fn tilde_fence_closes_and_paragraphs_are_partitioned_around_an_interrupting_fence() {
    let text = "before\n~~~js\ncode\n~~~\nafter";
    let result = parse(text);
    assert_eq!(
        result.leaves(),
        &[
            M11CleanLeaf::Paragraph {
                source: 0..7,
                source_utf16: 0..7,
                inline_source: 0..7,
                reference_definition_count: 0,
            },
            M11CleanLeaf::FencedCode {
                source: 7..22,
                source_utf16: 7..22,
                opening_marker: 7..10,
                raw_info_source: 10..12,
                body_source: 13..18,
                closing_marker: Some(18..21),
                marker: b'~',
                opening_indent: 0,
            },
            M11CleanLeaf::Paragraph {
                source: 22..27,
                source_utf16: 22..27,
                inline_source: 22..27,
                reference_definition_count: 0,
            },
        ]
    );
}

#[test]
fn only_a_matching_long_enough_unannotated_closer_ends_the_fence() {
    let text = "````\n```\n~~~~\n```` nope\n    ````\n  `````\t \r\nafter";
    let closing_line = text.rfind("  `````").expect("valid closer");
    let result = parse(text);
    assert_eq!(
        result.leaves(),
        &[
            M11CleanLeaf::FencedCode {
                source: 0..u32::try_from(closing_line + "  `````\t \r\n".len()).unwrap(),
                source_utf16: 0..u32::try_from(closing_line + "  `````\t \r\n".len()).unwrap(),
                opening_marker: 0..4,
                raw_info_source: 4..4,
                body_source: 5..u32::try_from(closing_line).unwrap(),
                closing_marker: Some(
                    u32::try_from(closing_line + 2).unwrap()
                        ..u32::try_from(closing_line + 7).unwrap(),
                ),
                marker: b'`',
                opening_indent: 0,
            },
            M11CleanLeaf::Paragraph {
                source: u32::try_from(closing_line + "  `````\t \r\n".len()).unwrap()
                    ..u32::try_from(text.len()).unwrap(),
                source_utf16: u32::try_from(closing_line + "  `````\t \r\n".len()).unwrap()
                    ..u32::try_from(text.len()).unwrap(),
                inline_source: u32::try_from(closing_line + "  `````\t \r\n".len()).unwrap()
                    ..u32::try_from(text.len()).unwrap(),
                reference_definition_count: 0,
            },
        ]
    );
}

#[test]
fn opening_indentation_zero_through_three_is_valid_but_four_is_not_a_fence() {
    for indent in 0..=3 {
        let spaces = " ".repeat(indent);
        let text = format!("{spaces}~~~ info\nbody\n{spaces}~~~\n");
        let result = parse(&text);
        let leaf = sole_fence(&result);
        let M11CleanLeaf::FencedCode {
            opening_marker,
            opening_indent,
            ..
        } = leaf
        else {
            unreachable!()
        };
        assert_eq!(*opening_indent, u8::try_from(indent).unwrap());
        assert_eq!(
            opening_marker.clone(),
            u32::try_from(indent).unwrap()..u32::try_from(indent + 3).unwrap()
        );
    }

    let result = parse("    ~~~\n");
    assert_eq!(
        result.leaves(),
        &[M11CleanLeaf::IndentedCode {
            source: 0..8,
            source_utf16: 0..8,
            line_count: 1,
            projected_utf8_length: 4,
            projected_utf16_length: 4,
            terminal_eol_bytes: 1,
            has_bof_bom: false,
        }]
    );
    assert!(!matches!(
        result.leaves(),
        [M11CleanLeaf::FencedCode { .. }]
    ));

    let invalid_backtick_info = parse("``` a`b\n");
    assert_eq!(
        invalid_backtick_info.kind(),
        M11CleanDocumentKind::Paragraph
    );
    assert!(!matches!(
        invalid_backtick_info.leaves(),
        [M11CleanLeaf::FencedCode { .. }]
    ));
}

#[test]
fn blank_markdown_and_reference_looking_lines_remain_literal_fence_body() {
    let text = "~~~\n\n# heading\n[x]: /url\n- list\n```\n~~~";
    let body_start = 4_u32;
    let closing_start = u32::try_from(text.rfind("~~~").expect("closer")).unwrap();
    let result = parse(text);
    let leaf = sole_fence(&result);
    let M11CleanLeaf::FencedCode {
        body_source,
        closing_marker,
        ..
    } = leaf
    else {
        unreachable!()
    };
    assert_eq!(body_source.clone(), body_start..closing_start);
    assert_eq!(
        closing_marker.clone(),
        Some(closing_start..closing_start + 3)
    );
    assert_eq!(
        &text[usize::try_from(body_source.start).unwrap()
            ..usize::try_from(body_source.end).unwrap()],
        "\n# heading\n[x]: /url\n- list\n```\n"
    );
}
