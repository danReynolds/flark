use flark_runtime::{
    DocumentBulletMarker, DocumentCodeBlockStyle, DocumentFenceCharacter, DocumentHeadingStyle,
    DocumentInlineFact, DocumentInlineFactKind, DocumentListDelimiter, DocumentListMarker,
    DocumentLiteralEditClass, DocumentLiveViewportSpan, DocumentProjectionEditCell,
    DocumentProjectionResultBlockShell, DocumentSession, DocumentSessionPhase, DocumentViewportRow,
    DocumentViewportRowEditCapability, DocumentViewportRowPresentation,
    DOCUMENT_PROJECTION_EDIT_CELL_BLOCK_PREFIX_PLAN_FLAGS,
    DOCUMENT_PROJECTION_EDIT_CELL_BLOCK_TRANSITION_FLAGS,
    DOCUMENT_PROJECTION_EDIT_CELL_EMPTY_LITERAL_RESULT,
    DOCUMENT_PROJECTION_EDIT_CELL_EXACT_SCALAR_FLAGS,
    DOCUMENT_PROJECTION_EDIT_CELL_LITERAL_APPEND_FLAGS,
    DOCUMENT_PROJECTION_EDIT_CELL_LITERAL_DELETE_ONE_FLAGS,
    DOCUMENT_PROJECTION_EDIT_CELL_LITERAL_WORD_FLAGS,
    DOCUMENT_PROJECTION_EDIT_CELL_PLAIN_ATX_FLAGS,
    DOCUMENT_PROJECTION_EDIT_CELL_STRONG_OPENING_SPACE_FLAGS,
    DOCUMENT_PROJECTION_EDIT_CELL_TERMINAL_SPACE_BLOCKED,
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

fn expected_utf16_range(source: &str, bytes: &std::ops::Range<u64>) -> std::ops::Range<u64> {
    let start = usize::try_from(bytes.start).expect("source-range start fits usize");
    let end = usize::try_from(bytes.end).expect("source-range end fits usize");
    u64::try_from(source[..start].encode_utf16().count()).expect("UTF-16 start fits u64")
        ..u64::try_from(source[..end].encode_utf16().count()).expect("UTF-16 end fits u64")
}

fn ordinary_projection_edit_cells(row: &DocumentViewportRow) -> Vec<DocumentProjectionEditCell> {
    row.projection_edit_cells
        .iter()
        .filter(|cell| {
            cell.flags != DOCUMENT_PROJECTION_EDIT_CELL_BLOCK_TRANSITION_FLAGS
                && cell.flags != DOCUMENT_PROJECTION_EDIT_CELL_BLOCK_PREFIX_PLAN_FLAGS
        })
        .cloned()
        .collect()
}

fn viewport_rows_without_pending_plans(
    mut rows: Vec<DocumentViewportRow>,
) -> Vec<DocumentViewportRow> {
    for row in &mut rows {
        row.pending_presentation_plans.clear();
    }
    rows
}

#[test]
fn frozen_fence_plans_publish_every_clean_prefix_result() {
    let cases = [
        (
            "change this line\n\n**sentinel**\n",
            "```dart\n",
            0usize,
            2u8,
        ),
        (
            "```dart\nchange this line\n\n**sentinel**\n",
            "\n```",
            24usize,
            1u8,
        ),
    ];
    for (source, sequence, trigger, replaced_row_count) in cases {
        let mut document = DocumentSession::begin(source).expect("begin fence plan base");
        pump_ready(&mut document);
        let viewport = document
            .query_viewport(document.revision(), 0..source.len(), 8)
            .expect("query fence plan base");
        let plans = viewport
            .rows
            .iter()
            .flat_map(|row| &row.pending_presentation_plans)
            .collect::<Vec<_>>();
        assert_eq!(plans.len(), 1);
        let plan = plans[0];
        assert_eq!(plan.sequence, sequence.as_bytes());
        assert_eq!(plan.trigger_range, trigger as u64..trigger as u64);
        assert_eq!(plan.trigger_utf16_range, trigger as u64..trigger as u64);
        assert_eq!(plan.replaced_row_count, replaced_row_count);
        assert_eq!(plan.steps.len(), sequence.len());

        for step in &plan.steps {
            let prefix_length = usize::from(step.prefix_length);
            let mut edited = source.to_owned();
            edited.insert_str(trigger, &sequence[..prefix_length]);
            let mut clean = DocumentSession::begin(&edited).expect("begin clean prefix");
            pump_ready(&mut clean);
            let clean_viewport = clean
                .query_viewport(clean.revision(), 0..edited.len(), 8)
                .expect("query clean prefix");
            assert_eq!(
                step.rows,
                viewport_rows_without_pending_plans(clean_viewport.rows),
                "prefix {prefix_length} for {sequence:?}"
            );
            for row in &step.rows {
                assert!(row.inline_facts.is_some(), "authoritative result facts");
                assert!(row.editable_range.is_some(), "result editable bytes");
                assert!(row.editable_utf16_range.is_some(), "result editable UTF-16");
                assert!(row.projection_segments.is_none());
                assert!(row.pending_presentation_plans.is_empty());
            }
            assert_eq!(step.affected_range, 0..edited.len() as u64);
            assert_eq!(
                step.affected_utf16_range,
                0..edited.encode_utf16().count() as u64
            );
            clean.close().expect("close clean prefix");
        }
        document.close().expect("close fence plan base");
    }
}

#[test]
fn simple_rows_publish_parser_authored_result_block_shell_transitions() {
    let cases = [
        (
            "#change\n",
            1..1,
            DocumentProjectionResultBlockShell::AtxHeading {
                level: 1,
                prefix_utf16_len: 2,
            },
            ' ',
        ),
        (
            "-change\n",
            1..1,
            DocumentProjectionResultBlockShell::ListItem {
                prefix_utf16_len: 2,
            },
            ' ',
        ),
        (
            "1.change\n",
            2..2,
            DocumentProjectionResultBlockShell::ListItem {
                prefix_utf16_len: 3,
            },
            ' ',
        ),
        (
            "change\n",
            0..0,
            DocumentProjectionResultBlockShell::BlockQuote {
                depth: 1,
                prefix_utf16_len: 1,
            },
            '>',
        ),
        (
            ">change\n",
            1..1,
            DocumentProjectionResultBlockShell::BlockQuote {
                depth: 1,
                prefix_utf16_len: 2,
            },
            ' ',
        ),
    ];
    for (source, trigger, shell, replacement) in cases {
        let mut document = DocumentSession::begin(source).expect("begin plain transition row");
        pump_ready(&mut document);
        let viewport = document
            .query_viewport(1, 0..source.len(), 8)
            .expect("plain transition viewport");
        let row = &viewport.rows[0];
        let cell = row
            .projection_edit_cells
            .iter()
            .find(|cell| {
                cell.flags == DOCUMENT_PROJECTION_EDIT_CELL_BLOCK_TRANSITION_FLAGS
                    && cell.trigger_range == trigger
                    && cell.result_block_shell == Some(shell)
            })
            .unwrap_or_else(|| panic!("missing result-shell transition: {source:?} {row:#?}"));
        assert_eq!(cell.source_range, 0..source.len() as u64 - 1);
        assert_eq!(cell.source_utf16_range, 0..source.len() as u64 - 1);
        assert_eq!(cell.replacement_first, u32::from(replacement));
        document.close().expect("close plain transition row");
        assert_block_transition_matches_clean(
            source,
            trigger.start as usize..trigger.end as usize,
            &replacement.to_string(),
            shell,
        );
        assert_single_edit_matches_clean(
            source,
            trigger.start as usize..trigger.end as usize,
            &replacement.to_string(),
        );
    }

    let removals = [
        (
            "# change\n",
            1..2,
            DocumentProjectionResultBlockShell::Plain {
                prefix_utf16_len: 0,
            },
        ),
        (
            "> change\n",
            1..2,
            DocumentProjectionResultBlockShell::BlockQuote {
                depth: 1,
                prefix_utf16_len: 1,
            },
        ),
        (
            "> change\n",
            0..1,
            DocumentProjectionResultBlockShell::Plain {
                prefix_utf16_len: 0,
            },
        ),
        (
            "- change\n",
            1..2,
            DocumentProjectionResultBlockShell::Plain {
                prefix_utf16_len: 0,
            },
        ),
        (
            "1. change\n",
            2..3,
            DocumentProjectionResultBlockShell::Plain {
                prefix_utf16_len: 0,
            },
        ),
    ];
    for (source, trigger, shell) in removals {
        let mut document = DocumentSession::begin(source).expect("begin block removal row");
        pump_ready(&mut document);
        let viewport = document
            .query_viewport(1, 0..source.len(), 8)
            .expect("block removal viewport");
        let row = &viewport.rows[0];
        let cell = row
            .projection_edit_cells
            .iter()
            .find(|cell| {
                cell.flags == DOCUMENT_PROJECTION_EDIT_CELL_BLOCK_TRANSITION_FLAGS
                    && cell.trigger_range == trigger
                    && cell.result_block_shell == Some(shell)
            })
            .unwrap_or_else(|| panic!("missing result-shell removal: {source:?} {row:#?}"));
        assert_eq!(cell.source_range, 0..source.len() as u64 - 1);
        assert_eq!(cell.source_utf16_range, 0..source.len() as u64 - 1);
        assert_eq!(cell.replacement_first, 0);
        document.close().expect("close block removal row");
        assert_block_transition_matches_clean(
            source,
            trigger.start as usize..trigger.end as usize,
            "",
            shell,
        );
        assert_single_edit_matches_clean(source, trigger.start as usize..trigger.end as usize, "");
    }

    let plans = [
        (
            "# ",
            2_u8,
            DocumentProjectionResultBlockShell::AtxHeading {
                level: 1,
                prefix_utf16_len: 2,
            },
        ),
        (
            "> ",
            1_u8,
            DocumentProjectionResultBlockShell::BlockQuote {
                depth: 1,
                prefix_utf16_len: 2,
            },
        ),
        (
            "- ",
            2_u8,
            DocumentProjectionResultBlockShell::ListItem {
                prefix_utf16_len: 2,
            },
        ),
        (
            "1. ",
            3_u8,
            DocumentProjectionResultBlockShell::ListItem {
                prefix_utf16_len: 3,
            },
        ),
    ];
    let source = "change\n";
    let mut base = DocumentSession::begin(source).expect("begin prefix-plan row");
    pump_ready(&mut base);
    let viewport = base
        .query_viewport(1, 0..source.len(), 8)
        .expect("prefix-plan viewport");
    let row = &viewport.rows[0];
    for (prefix, activation, shell) in plans {
        let packed = (u32::from(activation) << 24)
            | prefix
                .bytes()
                .enumerate()
                .fold(0_u32, |value, (index, byte)| {
                    value | (u32::from(byte) << (index * 8))
                });
        let cell = row
            .projection_edit_cells
            .iter()
            .find(|cell| {
                cell.flags == DOCUMENT_PROJECTION_EDIT_CELL_BLOCK_PREFIX_PLAN_FLAGS
                    && cell.trigger_range == (0..0)
                    && cell.replacement_first == packed
                    && cell.result_block_shell == Some(shell)
            })
            .unwrap_or_else(|| panic!("missing prefix plan {prefix:?}: {row:#?}"));
        assert_eq!(cell.source_range, 0..source.len() as u64 - 1);

        let mut incremental = DocumentSession::begin(source).expect("begin prefix-plan chain");
        pump_ready(&mut incremental);
        let mut edited = source.to_owned();
        for (index, character) in prefix.char_indices() {
            incremental
                .apply_edit(index as u64 + 1, index..index, &character.to_string())
                .expect("apply prefix-plan character");
            edited.insert(index, character);
            pump_ready(&mut incremental);
            assert_current_rows_match_clean(&mut incremental, index as u64 + 2, &edited);
        }
        incremental.close().expect("close prefix-plan chain");
    }
    base.close().expect("close prefix-plan row");
}

fn assert_block_transition_matches_clean(
    source: &str,
    range: std::ops::Range<usize>,
    replacement: &str,
    expected: DocumentProjectionResultBlockShell,
) {
    let mut edited = source.to_owned();
    edited.replace_range(range, replacement);
    let mut clean = DocumentSession::begin(&edited).expect("begin clean block-shell oracle");
    pump_ready(&mut clean);
    let viewport = clean
        .query_viewport(1, 0..edited.len(), 8)
        .expect("clean block-shell viewport");
    let actual = &viewport.rows[0].presentation;
    match expected {
        DocumentProjectionResultBlockShell::Plain { .. } => {
            assert_eq!(*actual, DocumentViewportRowPresentation::Plain)
        }
        DocumentProjectionResultBlockShell::AtxHeading { level, .. } => assert_eq!(
            *actual,
            DocumentViewportRowPresentation::Heading {
                level,
                style: DocumentHeadingStyle::Atx,
            }
        ),
        DocumentProjectionResultBlockShell::BlockQuote { depth, .. } => assert!(matches!(
            actual,
            DocumentViewportRowPresentation::BlockQuote {
                nesting_depth,
                ..
            } if *nesting_depth == depth
        )),
        DocumentProjectionResultBlockShell::ListItem { .. } => assert!(matches!(
            actual,
            DocumentViewportRowPresentation::ListItem {
                nesting_depth: 1,
                ..
            }
        )),
        DocumentProjectionResultBlockShell::Removed => {
            panic!("removed blocks use the dedicated zero-row assertion")
        }
    }
    clean.close().expect("close clean block-shell oracle");
}

fn assert_current_rows_match_clean(document: &mut DocumentSession, revision: u64, source: &str) {
    let incremental = document
        .query_viewport(revision, 0..source.len(), 64)
        .expect("incremental differential viewport");
    let mut clean = DocumentSession::begin(source).expect("begin clean differential oracle");
    pump_ready(&mut clean);
    let clean_viewport = clean
        .query_viewport(1, 0..source.len(), 64)
        .expect("clean differential viewport");
    assert_eq!(
        incremental.rows, clean_viewport.rows,
        "incremental rows must exactly match a fresh parse for {source:?}"
    );
    clean.close().expect("close clean differential oracle");
}

fn assert_single_edit_matches_clean(
    source: &str,
    range: std::ops::Range<usize>,
    replacement: &str,
) {
    let mut document = DocumentSession::begin(source).expect("begin single-edit differential");
    pump_ready(&mut document);
    document
        .apply_edit(1, range.clone(), replacement)
        .expect("apply single-edit differential");
    let mut edited_source = source.to_owned();
    edited_source.replace_range(range, replacement);
    pump_ready(&mut document);
    assert_current_rows_match_clean(&mut document, 2, &edited_source);
    document.close().expect("close single-edit differential");
}

fn shift_inline_fact(mut fact: DocumentInlineFact, delta: i64) -> DocumentInlineFact {
    fn shift(value: u64, delta: i64) -> u64 {
        if delta >= 0 {
            value + delta as u64
        } else {
            value - delta.unsigned_abs()
        }
    }

    fact.source_range = shift(fact.source_range.start, delta)..shift(fact.source_range.end, delta);
    fact.source_utf16_range =
        shift(fact.source_utf16_range.start, delta)..shift(fact.source_utf16_range.end, delta);
    fact.content_range =
        shift(fact.content_range.start, delta)..shift(fact.content_range.end, delta);
    fact.content_utf16_range =
        shift(fact.content_utf16_range.start, delta)..shift(fact.content_utf16_range.end, delta);
    fact
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
            item_end_byte: 19,
            item_end_utf16: 19,
            nesting_depth: 2,
            marker_offset: 0,
            item_padding: 2,
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
            container_widths: 2,
            container_count: 1,
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
            container_widths: 0x22,
            container_count: 2,
            simple_continuation: true,
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
            container_widths: 2,
            container_count: 1,
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
            container_widths: 2,
            container_count: 1,
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
        DocumentViewportRowEditCapability::ProjectedReserved,
    );
    assert_eq!(
        nested_multiline_viewport.rows[0]
            .projection_segments
            .as_ref()
            .expect("nested quote projection segments")
            .iter()
            .map(|segment| segment.source_range.clone())
            .collect::<Vec<_>>(),
        vec![4..10, 14..20],
    );
    nested_multiline
        .close()
        .expect("close nested multiline quote");

    let nested_boundary_source = "> > first\n> \n> second\n";
    let mut nested_boundary =
        DocumentSession::begin(nested_boundary_source).expect("begin nested quote boundary");
    pump_ready(&mut nested_boundary);
    let nested_boundary_viewport = nested_boundary
        .query_viewport(1, 0..nested_boundary_source.len(), 32)
        .expect("nested quote boundary viewport");
    assert_eq!(nested_boundary_viewport.rows.len(), 2);
    assert_eq!(
        nested_boundary_viewport.rows[1].presentation,
        DocumentViewportRowPresentation::BlockQuote {
            prefix_start_byte: 13,
            prefix_end_byte: 15,
            prefix_start_utf16: 13,
            prefix_end_utf16: 15,
            nesting_depth: 1,
            container_widths: 2,
            container_count: 1,
            simple_continuation: true,
        }
    );
    nested_boundary
        .close()
        .expect("close nested quote boundary");

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
    assert_eq!(
        viewport.rows[0].edit_capability,
        DocumentViewportRowEditCapability::Contiguous,
    );
    assert_eq!(viewport.rows[0].editable_range, Some(0..0));
    assert_eq!(viewport.rows[0].editable_utf16_range, Some(0..0));
    assert_eq!(viewport.rows[0].path_depth, 1);
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
                container_widths: 2,
                container_count: 1,
                simple_continuation: true,
            }
        );
        empty.close().expect("close empty BlockQuote");
    }
}

#[test]
fn fenced_code_body_word_cell_retains_the_code_shell_and_matches_clean_parse() {
    let source = "```dart\nfinal value = 'a';\n```\n";
    let mut document = DocumentSession::begin(source).expect("begin fenced code cell source");
    pump_ready(&mut document);
    let before = document
        .query_viewport(1, 0..source.len(), 32)
        .expect("fenced code cell viewport");
    let row = &before.rows[0];
    assert!(matches!(
        row.presentation,
        DocumentViewportRowPresentation::CodeBlock {
            style: DocumentCodeBlockStyle::Fenced { closed: true, .. },
        }
    ));
    assert!(row.projection_edit_cells.iter().any(|cell| {
        cell.flags == DOCUMENT_PROJECTION_EDIT_CELL_LITERAL_WORD_FLAGS
            && cell.source_range == (8..26)
            && cell.source_utf16_range == (8..26)
            && cell.trigger_range == (23..24)
            && cell.trigger_utf16_range == (23..24)
    }));
    assert!(row.projection_edit_cells.iter().any(|cell| {
        cell.flags == DOCUMENT_PROJECTION_EDIT_CELL_LITERAL_DELETE_ONE_FLAGS
            && cell.source_range == (8..26)
            && cell.source_utf16_range == (8..26)
            && cell.trigger_range == (23..24)
            && cell.trigger_utf16_range == (23..24)
    }));
    assert!(row.projection_edit_cells.iter().any(|cell| {
        cell.flags == DOCUMENT_PROJECTION_EDIT_CELL_LITERAL_APPEND_FLAGS
            && cell.source_range == (8..26)
            && cell.source_utf16_range == (8..26)
            && cell.trigger_range == (26..26)
            && cell.trigger_utf16_range == (26..26)
    }));

    document
        .apply_edit(1, 24..24, "x")
        .expect("edit fenced code body word");
    let edited = "```dart\nfinal value = 'ax';\n```\n";
    pump_ready(&mut document);
    assert_current_rows_match_clean(&mut document, 2, edited);
    let after = document
        .query_viewport(2, 0..edited.len(), 32)
        .expect("fenced code cell result viewport");
    assert!(matches!(
        after.rows[0].presentation,
        DocumentViewportRowPresentation::CodeBlock {
            style: DocumentCodeBlockStyle::Fenced { closed: true, .. },
        }
    ));
    document.close().expect("close fenced code cell source");
}

#[test]
fn fenced_code_single_literal_delete_cell_is_emitted_only_when_the_result_stays_code() {
    let safe_source = "```dart\nx\n```\n";
    let mut safe = DocumentSession::begin(safe_source).expect("begin safe fenced literal");
    pump_ready(&mut safe);
    let safe_viewport = safe
        .query_viewport(1, 0..safe_source.len(), 16)
        .expect("safe fenced literal viewport");
    assert!(safe_viewport.rows[0]
        .projection_edit_cells
        .iter()
        .any(|cell| {
            cell.flags == DOCUMENT_PROJECTION_EDIT_CELL_LITERAL_DELETE_ONE_FLAGS
                && cell.source_range == (8..9)
                && cell.trigger_range == (8..9)
        }));
    safe.close().expect("close safe fenced literal");

    let unsafe_source = "`````\n`````x\n`````\n";
    let mut unsafe_document =
        DocumentSession::begin(unsafe_source).expect("begin unsafe fenced literal");
    pump_ready(&mut unsafe_document);
    let unsafe_viewport = unsafe_document
        .query_viewport(1, 0..unsafe_source.len(), 16)
        .expect("unsafe fenced literal viewport");
    assert!(!unsafe_viewport.rows[0]
        .projection_edit_cells
        .iter()
        .any(|cell| {
            cell.flags == DOCUMENT_PROJECTION_EDIT_CELL_LITERAL_DELETE_ONE_FLAGS
                && cell.trigger_range == (11..12)
        }));
    unsafe_document
        .close()
        .expect("close unsafe fenced literal");
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
    let envelopes = &viewport.rows[0].literal_safe_envelopes;
    assert_eq!(
        envelopes
            .iter()
            .filter(|envelope| {
                envelope.edit_class == DocumentLiteralEditClass::AsciiWordInsertion
            })
            .count(),
        4,
        "only parser-proven inline content publishes word insertion envelopes"
    );
    assert!(
        envelopes.iter().all(|envelope| {
            envelope.edit_class != DocumentLiteralEditClass::SingleAsciiSpaceInsertion
        }),
        "a row whose final construct is an autolink has no trailing-space proof"
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

    let mut boundary = DocumentSession::begin("*test*\n").expect("begin boundary document");
    pump_ready(&mut boundary);
    let boundary_viewport = boundary
        .query_viewport(1, 0..7, 8)
        .expect("boundary viewport");
    assert_eq!(
        boundary_viewport.rows[0].literal_safe_envelopes,
        vec![
            flark_runtime::DocumentLiteralSafeEnvelope {
                edit_class: DocumentLiteralEditClass::AsciiWordInsertion,
                source_range: 1..5,
                source_utf16_range: 1..5,
            },
            flark_runtime::DocumentLiteralSafeEnvelope {
                edit_class: DocumentLiteralEditClass::SingleAsciiLiteralUnitDeletion,
                source_range: 1..5,
                source_utf16_range: 1..5,
            },
            flark_runtime::DocumentLiteralSafeEnvelope {
                edit_class: DocumentLiteralEditClass::SingleAsciiSpaceInsertion,
                source_range: 6..6,
                source_utf16_range: 6..6,
            },
        ]
    );
    boundary.close().expect("close boundary document");

    let nested_source = "*a _b_ c*\n";
    let mut nested = DocumentSession::begin(nested_source).expect("begin nested document");
    pump_ready(&mut nested);
    let nested_viewport = nested
        .query_viewport(1, 0..nested_source.len(), 8)
        .expect("nested viewport");
    let nested_opening = nested_source.find('_').expect("nested opening") as u64;
    assert!(
        nested_viewport.rows[0]
            .literal_safe_envelopes
            .iter()
            .filter(|envelope| {
                envelope.edit_class == DocumentLiteralEditClass::AsciiWordInsertion
            })
            .all(|envelope| {
                !(envelope.source_range.start <= nested_opening
                    && nested_opening <= envelope.source_range.end)
            }),
        "an outer fact must not authorize insertion at a nested delimiter boundary"
    );
    assert!(
        nested_viewport.rows[0]
            .literal_safe_envelopes
            .iter()
            .any(|envelope| {
                envelope.edit_class == DocumentLiteralEditClass::AsciiWordInsertion
                    && envelope.source_range == ((nested_opening + 1)..(nested_opening + 2))
            }),
        "the parser may still authorize the nested flat word itself"
    );
    nested.close().expect("close nested document");

    let latent_source = "*a &am; z*\n";
    let mut latent = DocumentSession::begin(latent_source).expect("begin latent syntax document");
    pump_ready(&mut latent);
    let latent_viewport = latent
        .query_viewport(1, 0..latent_source.len(), 8)
        .expect("latent syntax viewport");
    let entity_completion = latent_source.find(';').expect("latent entity terminator") as u64;
    assert!(
        latent_viewport.rows[0]
            .literal_safe_envelopes
            .iter()
            .filter(|envelope| {
                envelope.edit_class == DocumentLiteralEditClass::AsciiWordInsertion
            })
            .all(|envelope| {
                !(envelope.source_range.start <= entity_completion
                    && entity_completion <= envelope.source_range.end)
            }),
        "plain-looking punctuation that can become syntax must not be certified"
    );
    latent.close().expect("close latent syntax document");

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
fn canonical_plain_atx_heading_publishes_one_bounded_projection_edit_cell() {
    assert_eq!(DOCUMENT_PROJECTION_EDIT_CELL_PLAIN_ATX_FLAGS, 0x0d01);
    let source = "# Café—road\n";
    let mut document = DocumentSession::begin(source).expect("begin Unicode ATX heading");
    pump_ready(&mut document);
    let viewport = document
        .query_viewport(1, 0..source.len(), 8)
        .expect("Unicode ATX heading viewport");
    let row = &viewport.rows[0];
    assert_eq!(
        ordinary_projection_edit_cells(row),
        vec![DocumentProjectionEditCell {
            source_range: 2..14,
            source_utf16_range: 2..11,
            trigger_range: 2..14,
            trigger_utf16_range: 2..11,
            flags: DOCUMENT_PROJECTION_EDIT_CELL_PLAIN_ATX_FLAGS,
            replacement_first: 0,
            replacement_second: 0,
            result_block_shell: None,
        }]
    );
    assert!(
        row.literal_safe_envelopes.is_empty(),
        "the edit cell replaces the old whole-heading literal envelopes"
    );
    document.close().expect("close Unicode ATX heading");

    for empty_source in ["# ", "# \n", "###### \r\n"] {
        let mut empty = DocumentSession::begin(empty_source).expect("begin empty ATX heading");
        pump_ready(&mut empty);
        let viewport = empty
            .query_viewport(1, 0..empty_source.len(), 8)
            .expect("empty ATX heading viewport");
        let row = &viewport.rows[0];
        let ordinary = ordinary_projection_edit_cells(row);
        let [cell] = ordinary.as_slice() else {
            panic!("empty canonical heading edit cell: {empty_source:?} {row:#?}");
        };
        assert!(cell.source_range.is_empty());
        assert!(cell.source_utf16_range.is_empty());
        assert_eq!(cell.source_range, row.editable_range.clone().unwrap());
        assert_eq!(
            cell.source_utf16_range,
            row.editable_utf16_range.clone().unwrap()
        );
        empty.close().expect("close empty ATX heading");
    }

    let later_source = "Paragraph\n# Heading\n";
    let mut later = DocumentSession::begin(later_source).expect("begin later top-level heading");
    pump_ready(&mut later);
    let viewport = later
        .query_viewport(1, 0..later_source.len(), 8)
        .expect("later top-level heading viewport");
    assert!(
        viewport
            .rows
            .iter()
            .any(|row| !ordinary_projection_edit_cells(row).is_empty()),
        "a prior physical line must not prevent a top-level cell"
    );
    later.close().expect("close later top-level heading");
}

#[test]
fn flat_strong_opening_space_cell_retains_shifted_outside_facts() {
    assert_eq!(
        DOCUMENT_PROJECTION_EDIT_CELL_STRONG_OPENING_SPACE_FLAGS,
        0x0703
    );
    let source = "# **left** middle _right_\n";
    let mut document = DocumentSession::begin(source).expect("begin mixed-inline ATX heading");
    pump_ready(&mut document);
    let viewport = document
        .query_viewport(1, 0..source.len(), 8)
        .expect("mixed-inline ATX heading viewport");
    let row = &viewport.rows[0];
    assert_eq!(
        ordinary_projection_edit_cells(row),
        vec![DocumentProjectionEditCell {
            source_range: 2..10,
            source_utf16_range: 2..10,
            trigger_range: 4..4,
            trigger_utf16_range: 4..4,
            flags: DOCUMENT_PROJECTION_EDIT_CELL_STRONG_OPENING_SPACE_FLAGS,
            replacement_first: 0,
            replacement_second: 0,
            result_block_shell: None,
        }]
    );
    let facts = row
        .inline_facts
        .as_ref()
        .expect("authoritative mixed-inline facts");
    assert_eq!(
        facts.iter().map(|fact| fact.kind).collect::<Vec<_>>(),
        vec![
            DocumentInlineFactKind::Strong,
            DocumentInlineFactKind::Emphasis,
        ]
    );
    let before_outside = facts[1].clone();
    assert_eq!(before_outside.source_range, 18..25);
    assert_eq!(before_outside.content_range, 19..24);
    assert!(
        row.literal_safe_envelopes.iter().any(|envelope| {
            envelope.edit_class == DocumentLiteralEditClass::SingleAsciiAsteriskInsertion
                && envelope.source_range == (4..8)
        }),
        "the isolated Strong content must publish one-shot asterisk authority"
    );

    document
        .apply_edit(1, 4..4, " ")
        .expect("insert one opening space");
    pump_ready(&mut document);
    let edited_source = source.replacen("**left**", "** left**", 1);
    assert_current_rows_match_clean(&mut document, 2, &edited_source);
    let viewport = document
        .query_viewport(2, 0..source.len() + 1, 8)
        .expect("post-edit mixed-inline ATX heading viewport");
    let row = &viewport.rows[0];
    assert_eq!(
        row.presentation,
        DocumentViewportRowPresentation::Heading {
            level: 1,
            style: DocumentHeadingStyle::Atx,
        }
    );
    let after_outside = row
        .inline_facts
        .as_ref()
        .expect("post-edit authoritative inline facts")
        .iter()
        .find(|fact| fact.kind == DocumentInlineFactKind::Emphasis)
        .expect("outside emphasis remains rendered");
    assert_eq!(after_outside.kind, before_outside.kind);
    assert_eq!(after_outside.flags, before_outside.flags);
    assert_eq!(after_outside.replacement, before_outside.replacement);
    assert_eq!(
        after_outside.source_range,
        before_outside.source_range.start + 1..before_outside.source_range.end + 1
    );
    assert_eq!(
        after_outside.source_utf16_range,
        before_outside.source_utf16_range.start + 1..before_outside.source_utf16_range.end + 1
    );
    assert_eq!(
        after_outside.content_range,
        before_outside.content_range.start + 1..before_outside.content_range.end + 1
    );
    assert_eq!(
        after_outside.content_utf16_range,
        before_outside.content_utf16_range.start + 1..before_outside.content_utf16_range.end + 1
    );
    document.close().expect("close mixed-inline ATX heading");
}

#[test]
fn parser_parameterized_bracket_cell_keeps_the_strong_dependency_local() {
    assert_eq!(DOCUMENT_PROJECTION_EDIT_CELL_EXACT_SCALAR_FLAGS, 0x0706);
    let source = "Before **bold** after.\n";
    let mut document = DocumentSession::begin(source).expect("begin bracket component");
    pump_ready(&mut document);
    let viewport = document
        .query_viewport(1, 0..source.len(), 8)
        .expect("bracket component viewport");
    let row = &viewport.rows[0];
    let exact_cells = row
        .projection_edit_cells
        .iter()
        .filter(|cell| {
            cell.flags == DOCUMENT_PROJECTION_EDIT_CELL_EXACT_SCALAR_FLAGS
                && cell.replacement_first == u32::from('[')
                && cell.source_range == (7..15)
        })
        .collect::<Vec<_>>();
    assert_eq!(exact_cells.len(), 3);
    assert_eq!(
        exact_cells
            .iter()
            .map(|cell| (
                cell.source_range.clone(),
                cell.source_utf16_range.clone(),
                cell.trigger_range.clone(),
                cell.trigger_utf16_range.clone(),
                cell.replacement_first,
                cell.replacement_second,
            ))
            .collect::<Vec<_>>(),
        vec![
            (7..15, 7..15, 10..10, 10..10, u32::from('['), 0),
            (7..15, 7..15, 11..11, 11..11, u32::from('['), 0),
            (7..15, 7..15, 12..12, 12..12, u32::from('['), 0),
        ]
    );

    for point in [10_usize, 11, 12] {
        assert_single_edit_matches_clean(source, point..point, "[");
    }

    document
        .apply_edit(1, 11..11, "[")
        .expect("insert bracket inside Strong content");
    pump_ready(&mut document);
    let edited_source = "Before **bo[ld** after.\n";
    assert_current_rows_match_clean(&mut document, 2, edited_source);
    let edited = document
        .query_viewport(2, 0..edited_source.len(), 8)
        .expect("edited bracket component viewport");
    let facts = edited.rows[0]
        .inline_facts
        .as_ref()
        .expect("edited inline facts");
    assert_eq!(facts.len(), 1);
    assert_eq!(facts[0].kind, DocumentInlineFactKind::Strong);
    assert_eq!(facts[0].source_range, 7..16);
    assert_eq!(facts[0].content_range, 9..14);
    document.close().expect("close bracket component");

    let unsafe_source = "Before **bold** after ].\n";
    let mut unsafe_document =
        DocumentSession::begin(unsafe_source).expect("begin bracket dependency negative");
    pump_ready(&mut unsafe_document);
    let unsafe_viewport = unsafe_document
        .query_viewport(1, 0..unsafe_source.len(), 8)
        .expect("bracket dependency negative viewport");
    assert!(unsafe_viewport.rows[0]
        .projection_edit_cells
        .iter()
        .all(|cell| {
            cell.flags != DOCUMENT_PROJECTION_EDIT_CELL_EXACT_SCALAR_FLAGS
                || cell.replacement_first != u32::from('[')
        }));
    unsafe_document
        .close()
        .expect("close bracket dependency negative");
}

#[test]
fn guarded_plain_prefix_punctuation_cells_preserve_the_outside_strong_fact() {
    let source = "AlphaBeta and **bold**.\n";
    let point = 5_usize;
    let scalars = [
        '.', ',', ';', ':', '!', '?', '\'', '"', '(', ')', '-', '–', '—',
    ];
    let mut document = DocumentSession::begin(source).expect("begin guarded punctuation source");
    pump_ready(&mut document);
    let viewport = document
        .query_viewport(1, 0..source.len(), 8)
        .expect("guarded punctuation viewport");
    let row = &viewport.rows[0];
    let declared = row
        .projection_edit_cells
        .iter()
        .filter_map(|cell| {
            (cell.flags == DOCUMENT_PROJECTION_EDIT_CELL_EXACT_SCALAR_FLAGS
                && cell.source_range == (0..14)
                && cell.trigger_range == (point as u64..point as u64))
                .then_some(char::from_u32(cell.replacement_first))
                .flatten()
        })
        .filter(|scalar| scalars.contains(scalar))
        .collect::<Vec<_>>();
    assert_eq!(declared, scalars);
    document.close().expect("close guarded punctuation source");

    for scalar in scalars {
        let mut document = DocumentSession::begin(source).expect("begin guarded punctuation edit");
        pump_ready(&mut document);
        let before = document
            .query_viewport(1, 0..source.len(), 8)
            .expect("guarded punctuation before viewport")
            .rows[0]
            .inline_facts
            .as_ref()
            .expect("guarded punctuation before facts")
            .iter()
            .find(|fact| fact.kind == DocumentInlineFactKind::Strong)
            .expect("guarded punctuation before Strong")
            .clone();
        let replacement = scalar.to_string();
        document
            .apply_edit(1, point..point, &replacement)
            .expect("apply guarded punctuation");
        let mut edited_source = source.to_owned();
        edited_source.insert(point, scalar);
        pump_ready(&mut document);
        assert_current_rows_match_clean(&mut document, 2, &edited_source);
        let after = document
            .query_viewport(2, 0..edited_source.len(), 8)
            .expect("guarded punctuation after viewport")
            .rows[0]
            .inline_facts
            .as_ref()
            .expect("guarded punctuation after facts")
            .iter()
            .find(|fact| fact.kind == DocumentInlineFactKind::Strong)
            .expect("guarded punctuation after Strong")
            .clone();
        let byte_delta = scalar.len_utf8() as u64;
        let utf16_delta = scalar.len_utf16() as u64;
        assert_eq!(after.kind, before.kind, "{scalar:?}");
        assert_eq!(after.flags, before.flags, "{scalar:?}");
        assert_eq!(after.replacement, before.replacement, "{scalar:?}");
        assert_eq!(
            after.source_range,
            before.source_range.start + byte_delta..before.source_range.end + byte_delta,
            "{scalar:?}",
        );
        assert_eq!(
            after.content_range,
            before.content_range.start + byte_delta..before.content_range.end + byte_delta,
            "{scalar:?}",
        );
        assert_eq!(
            after.source_utf16_range,
            before.source_utf16_range.start + utf16_delta
                ..before.source_utf16_range.end + utf16_delta,
            "{scalar:?}",
        );
        assert_eq!(
            after.content_utf16_range,
            before.content_utf16_range.start + utf16_delta
                ..before.content_utf16_range.end + utf16_delta,
            "{scalar:?}",
        );
        document.close().expect("close guarded punctuation edit");
    }
}

#[test]
fn guarded_plain_prefix_syntax_cells_preserve_the_different_marker_sibling() {
    for (source, scalar, outside_kind) in [
        ("abcd _right_\n", '*', DocumentInlineFactKind::Emphasis),
        ("abcd **right**\n", '_', DocumentInlineFactKind::Strong),
        ("abcd **right**\n", '~', DocumentInlineFactKind::Strong),
        ("abcd _right_\n", '`', DocumentInlineFactKind::Emphasis),
        ("abcd _right_\n", '[', DocumentInlineFactKind::Emphasis),
        ("abcd _right_\n", ']', DocumentInlineFactKind::Emphasis),
    ] {
        let mut document = DocumentSession::begin(source).expect("begin guarded syntax source");
        pump_ready(&mut document);
        let before_viewport = document
            .query_viewport(1, 0..source.len(), 8)
            .expect("guarded syntax before viewport");
        let row = &before_viewport.rows[0];
        assert!(row.projection_edit_cells.iter().any(|cell| {
            cell.flags == DOCUMENT_PROJECTION_EDIT_CELL_EXACT_SCALAR_FLAGS
                && cell.source_range == (0..5)
                && cell.trigger_range == (2..2)
                && cell.replacement_first == u32::from(scalar)
        }));
        let before = row
            .inline_facts
            .as_ref()
            .expect("guarded syntax before facts")
            .iter()
            .find(|fact| fact.kind == outside_kind)
            .expect("guarded syntax outside fact")
            .clone();

        let replacement = scalar.to_string();
        document
            .apply_edit(1, 2..2, &replacement)
            .expect("apply guarded syntax scalar");
        let mut edited_source = source.to_owned();
        edited_source.insert(2, scalar);
        pump_ready(&mut document);
        assert_current_rows_match_clean(&mut document, 2, &edited_source);
        let after = document
            .query_viewport(2, 0..edited_source.len(), 8)
            .expect("guarded syntax after viewport")
            .rows[0]
            .inline_facts
            .as_ref()
            .expect("guarded syntax after facts")
            .iter()
            .find(|fact| fact.kind == outside_kind)
            .expect("guarded syntax retained outside fact")
            .clone();
        assert_eq!(after.kind, before.kind, "{scalar:?}");
        assert_eq!(after.flags, before.flags, "{scalar:?}");
        assert_eq!(after.replacement, before.replacement, "{scalar:?}");
        assert_eq!(
            after.source_range,
            before.source_range.start + 1..before.source_range.end + 1,
            "{scalar:?}",
        );
        assert_eq!(
            after.content_range,
            before.content_range.start + 1..before.content_range.end + 1,
            "{scalar:?}",
        );
        assert_eq!(
            after.source_utf16_range,
            before.source_utf16_range.start + 1..before.source_utf16_range.end + 1,
            "{scalar:?}",
        );
        assert_eq!(
            after.content_utf16_range,
            before.content_utf16_range.start + 1..before.content_utf16_range.end + 1,
            "{scalar:?}",
        );
        document.close().expect("close guarded syntax source");
    }
}

#[test]
fn flat_strong_asterisk_envelope_preserves_the_certified_fact() {
    let source = "Before **bold** and _right_.\n";
    let mut document = DocumentSession::begin(source).expect("begin Strong dependency cell");
    pump_ready(&mut document);
    let viewport = document
        .query_viewport(1, 0..source.len(), 8)
        .expect("Strong dependency viewport");
    let row = &viewport.rows[0];
    let envelope = row
        .literal_safe_envelopes
        .iter()
        .find(|envelope| {
            envelope.edit_class == DocumentLiteralEditClass::SingleAsciiAsteriskInsertion
        })
        .expect("parser-authored Strong asterisk envelope");
    assert_eq!(envelope.source_range, 9..13);
    assert_eq!(envelope.source_utf16_range, 9..13);
    let before_emphasis = row
        .inline_facts
        .as_ref()
        .expect("base inline facts")
        .iter()
        .find(|fact| fact.kind == DocumentInlineFactKind::Emphasis)
        .expect("outside Emphasis")
        .clone();

    document
        .apply_edit(1, 11..11, "*")
        .expect("insert one asterisk inside Strong content");
    pump_ready(&mut document);
    let edited_source = "Before **bo*ld** and _right_.\n";
    assert_current_rows_match_clean(&mut document, 2, edited_source);
    let viewport = document
        .query_viewport(2, 0..edited_source.len(), 8)
        .expect("edited Strong dependency viewport");
    let after_emphasis = viewport.rows[0]
        .inline_facts
        .as_ref()
        .expect("edited inline facts")
        .iter()
        .find(|fact| fact.kind == DocumentInlineFactKind::Emphasis)
        .expect("outside Emphasis remains projected");
    assert_eq!(after_emphasis.kind, before_emphasis.kind);
    assert_eq!(after_emphasis.flags, before_emphasis.flags);
    assert_eq!(after_emphasis.replacement, before_emphasis.replacement);
    assert_eq!(
        after_emphasis.source_range,
        before_emphasis.source_range.start + 1..before_emphasis.source_range.end + 1
    );
    assert_eq!(
        after_emphasis.content_range,
        before_emphasis.content_range.start + 1..before_emphasis.content_range.end + 1
    );
    document.close().expect("close Strong dependency cell");
}

#[test]
fn inline_code_word_publishes_one_shot_deletion_authority() {
    let source = "Before `code`, after.\n";
    let mut document = DocumentSession::begin(source).expect("begin inline code deletion");
    pump_ready(&mut document);
    let viewport = document
        .query_viewport(1, 0..source.len(), 8)
        .expect("inline code deletion viewport");
    let row = &viewport.rows[0];
    let matching = row
        .literal_safe_envelopes
        .iter()
        .filter(|envelope| envelope.source_range == (8..12))
        .map(|envelope| envelope.edit_class)
        .collect::<Vec<_>>();
    assert_eq!(
        matching,
        vec![
            DocumentLiteralEditClass::AsciiWordInsertion,
            DocumentLiteralEditClass::SingleAsciiLiteralUnitDeletion,
        ]
    );
    assert!(row.literal_safe_envelopes.iter().any(|envelope| {
        envelope.edit_class == DocumentLiteralEditClass::SingleAsciiLiteralUnitDeletion
            && envelope.source_range == (13..14)
            && envelope.source_utf16_range == (13..14)
    }));

    document
        .apply_edit(1, 11..12, "")
        .expect("delete one inline code word unit");
    pump_ready(&mut document);
    assert_current_rows_match_clean(&mut document, 2, "Before `cod`, after.\n");
    assert!(document
        .query_viewport(2, 0..source.len() - 1, 8)
        .expect("edited inline code viewport")
        .rows[0]
        .inline_facts
        .as_ref()
        .expect("edited inline facts")
        .iter()
        .any(|fact| fact.kind == DocumentInlineFactKind::Code));
    document.close().expect("close inline code deletion");
}

#[test]
fn sole_line_punctuation_withholds_literal_deletion_authority() {
    let source = "1. outer\n   - inne\n.\n\n**sentinel**\n";
    let mut document = DocumentSession::begin(source).expect("begin continuation punctuation");
    pump_ready(&mut document);
    let viewport = document
        .query_viewport(1, 0..source.len(), 8)
        .expect("continuation punctuation viewport");
    assert!(viewport.rows.iter().all(|row| {
        row.literal_safe_envelopes.iter().all(|envelope| {
            envelope.edit_class != DocumentLiteralEditClass::SingleAsciiLiteralUnitDeletion
                || envelope.source_range != (19..20)
        })
    }));
    document.close().expect("close continuation punctuation");
}

#[test]
fn literal_safe_word_envelopes_are_bounded_without_dropping_inline_facts() {
    let content = std::iter::repeat("a")
        .take(200)
        .collect::<Vec<_>>()
        .join(" ");
    let source = format!("**{content}**\n");
    let mut document = DocumentSession::begin(&source).expect("begin word-dense Strong row");
    pump_ready(&mut document);
    let viewport = document
        .query_viewport(1, 0..source.len(), 8)
        .expect("word-dense Strong viewport");
    let row = &viewport.rows[0];
    let facts = row
        .inline_facts
        .as_ref()
        .expect("word-density must not discard authoritative inline facts");
    assert!(
        facts
            .iter()
            .any(|fact| fact.kind == DocumentInlineFactKind::Strong),
        "the projected Strong fact remains authoritative"
    );
    assert_eq!(
        row.literal_safe_envelopes.len(),
        128,
        "the optimization must stay within its per-row ABI payload budget"
    );
    assert!(row
        .literal_safe_envelopes
        .iter()
        .all(|envelope| { envelope.edit_class == DocumentLiteralEditClass::AsciiWordInsertion }));
    document.close().expect("close word-dense Strong row");
}

#[test]
fn plain_literal_segments_publish_chainable_splice_and_one_shot_delete_cells() {
    assert_eq!(DOCUMENT_PROJECTION_EDIT_CELL_LITERAL_WORD_FLAGS, 0x0f02);
    assert_eq!(DOCUMENT_PROJECTION_EDIT_CELL_LITERAL_APPEND_FLAGS, 0x0f05);
    assert_eq!(
        DOCUMENT_PROJECTION_EDIT_CELL_LITERAL_DELETE_ONE_FLAGS,
        0x0704
    );
    let source = "This is the real **Rust → Dart → Flutter** editor path.\n";
    let mut document = DocumentSession::begin(source).expect("begin dogfood paragraph");
    pump_ready(&mut document);
    let viewport = document
        .query_viewport(1, 0..source.len(), 8)
        .expect("dogfood paragraph viewport");
    let row = &viewport.rows[0];
    let facts = row
        .inline_facts
        .as_ref()
        .expect("authoritative paragraph inline facts");
    let strong = facts
        .iter()
        .find(|fact| fact.kind == DocumentInlineFactKind::Strong)
        .expect("Strong fact");
    assert_eq!(strong.source_range, 17..46);
    assert_eq!(strong.source_utf16_range, 17..42);
    assert_eq!(row.editable_range, Some(0..59));
    assert_eq!(row.editable_utf16_range, Some(0..55));
    assert!(
        row.literal_safe_envelopes.iter().any(|envelope| {
            envelope.edit_class == DocumentLiteralEditClass::AsciiWordInsertion
                && envelope.source_range
                    == (strong.content_range.start..strong.content_range.start + 4)
                && envelope.source_utf16_range
                    == (strong.content_utf16_range.start..strong.content_utf16_range.start + 4)
        }),
        "plain prose cells must coexist with parser-authored Strong leaf authority"
    );
    assert_eq!(
        row.projection_edit_cells
            .iter()
            .filter(|cell| {
                cell.source_range.start == 0
                    && matches!(
                        cell.flags,
                        DOCUMENT_PROJECTION_EDIT_CELL_LITERAL_WORD_FLAGS
                            | DOCUMENT_PROJECTION_EDIT_CELL_LITERAL_DELETE_ONE_FLAGS
                    )
            })
            .cloned()
            .collect::<Vec<_>>(),
        vec![
            DocumentProjectionEditCell {
                source_range: 0..17,
                source_utf16_range: 0..17,
                trigger_range: 0..16,
                trigger_utf16_range: 0..16,
                flags: DOCUMENT_PROJECTION_EDIT_CELL_LITERAL_WORD_FLAGS,
                replacement_first: 0,
                replacement_second: 0,
                result_block_shell: None,
            },
            DocumentProjectionEditCell {
                source_range: 0..17,
                source_utf16_range: 0..17,
                trigger_range: 0..16,
                trigger_utf16_range: 0..16,
                flags: DOCUMENT_PROJECTION_EDIT_CELL_LITERAL_DELETE_ONE_FLAGS,
                replacement_first: 0,
                replacement_second: 0,
                result_block_shell: None,
            },
        ],
        "the legacy plain-prefix cells keep their exact geometry"
    );
    let tail = row
        .projection_edit_cells
        .iter()
        .find(|cell| {
            cell.flags == DOCUMENT_PROJECTION_EDIT_CELL_LITERAL_WORD_FLAGS
                && cell.source_range == (46..59)
        })
        .expect("parser-authored punctuation-bounded tail cell");
    assert_eq!(tail.trigger_range, 48..57);
    assert_eq!(
        &source[tail.source_range.start as usize..tail.source_range.end as usize],
        " editor path."
    );
    document.close().expect("close dogfood paragraph");
}

#[test]
fn deleting_the_only_plain_literal_unit_publishes_removed_block_shell() {
    let source = "x\n\nnext\n";
    let mut document = DocumentSession::begin(source).expect("begin one-unit paragraph");
    pump_ready(&mut document);
    let viewport = document
        .query_viewport(1, 0..source.len(), 8)
        .expect("one-unit paragraph viewport");
    let row = &viewport.rows[0];
    assert_eq!(row.presentation, DocumentViewportRowPresentation::Plain);
    assert!(row.projection_edit_cells.iter().any(|cell| {
        cell.source_range == (0..1)
            && cell.source_utf16_range == (0..1)
            && cell.trigger_range == (0..1)
            && cell.trigger_utf16_range == (0..1)
            && cell.flags == DOCUMENT_PROJECTION_EDIT_CELL_BLOCK_TRANSITION_FLAGS
            && cell.result_block_shell == Some(DocumentProjectionResultBlockShell::Removed)
    }));
    document.close().expect("close one-unit paragraph");
}

#[test]
fn multiline_product_paragraph_publishes_only_physical_line_literal_cells() {
    let source = "This is the real **Rust → Dart → Flutter** editor path. Use it like an editor,\n\
not a static Markdown preview. Certified Markdown stays rendered while focused;\n\
only incomplete or temporarily pending syntax becomes exact source locally.\n";
    let mut document = DocumentSession::begin(source).expect("begin product paragraph");
    pump_ready(&mut document);
    let viewport = document
        .query_viewport(1, 0..source.len(), 8)
        .expect("product paragraph viewport");
    let row = &viewport.rows[0];
    assert_eq!(row.presentation, DocumentViewportRowPresentation::Plain);
    assert_eq!(row.editable_range, Some(0..source.len() as u64 - 1));
    let legacy_cells = row
        .projection_edit_cells
        .iter()
        .filter(|cell| {
            (cell.source_range == (0..17)
                && matches!(
                    cell.flags,
                    DOCUMENT_PROJECTION_EDIT_CELL_LITERAL_WORD_FLAGS
                        | DOCUMENT_PROJECTION_EDIT_CELL_LITERAL_DELETE_ONE_FLAGS
                ))
                || cell.flags == DOCUMENT_PROJECTION_EDIT_CELL_LITERAL_APPEND_FLAGS
        })
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(
        legacy_cells,
        vec![
            DocumentProjectionEditCell {
                source_range: 0..17,
                source_utf16_range: 0..17,
                trigger_range: 0..16,
                trigger_utf16_range: 0..16,
                flags: DOCUMENT_PROJECTION_EDIT_CELL_LITERAL_WORD_FLAGS,
                replacement_first: 0,
                replacement_second: 0,
                result_block_shell: None,
            },
            DocumentProjectionEditCell {
                source_range: 0..17,
                source_utf16_range: 0..17,
                trigger_range: 0..16,
                trigger_utf16_range: 0..16,
                flags: DOCUMENT_PROJECTION_EDIT_CELL_LITERAL_DELETE_ONE_FLAGS,
                replacement_first: 0,
                replacement_second: 0,
                result_block_shell: None,
            },
            DocumentProjectionEditCell {
                source_range: 163..238,
                source_utf16_range: 159..234,
                trigger_range: 238..238,
                trigger_utf16_range: 234..234,
                flags: DOCUMENT_PROJECTION_EDIT_CELL_LITERAL_APPEND_FLAGS,
                replacement_first: 0,
                replacement_second: 0,
                result_block_shell: None,
            },
        ],
        "the legacy prefix and terminal cells keep their exact geometry",
    );

    let phrase_start = source.find("temporarily pending").expect("paste phrase");
    let phrase_end = phrase_start + "temporarily pending".len();
    let prose_cell = row
        .projection_edit_cells
        .iter()
        .find(|cell| {
            cell.flags == DOCUMENT_PROJECTION_EDIT_CELL_LITERAL_WORD_FLAGS
                && cell.trigger_range.start < phrase_start as u64
                && (phrase_end as u64) < cell.trigger_range.end
        })
        .expect("parser-authored guarded multiword cell");
    assert_eq!(
        &source[prose_cell.source_range.start as usize..prose_cell.source_range.end as usize],
        "only incomplete or temporarily pending syntax becomes exact source locally."
    );
    let before_strong = row
        .inline_facts
        .as_ref()
        .expect("product facts")
        .iter()
        .find(|fact| fact.kind == DocumentInlineFactKind::Strong)
        .expect("outside Strong fact")
        .clone();

    document
        .apply_edit(1, phrase_start..phrase_end, "briefly pending")
        .expect("apply guarded multiword paste");
    let edited_source = source.replacen("temporarily pending", "briefly pending", 1);
    pump_ready(&mut document);
    assert_current_rows_match_clean(&mut document, 2, &edited_source);
    let edited = document
        .query_viewport(2, 0..edited_source.len(), 8)
        .expect("edited product paragraph viewport");
    let after_strong = edited.rows[0]
        .inline_facts
        .as_ref()
        .expect("edited product facts")
        .iter()
        .find(|fact| fact.kind == DocumentInlineFactKind::Strong)
        .expect("retained Strong fact");
    assert_eq!(after_strong, &before_strong);
    document.close().expect("close product paragraph");

    assert_single_edit_matches_clean(source, 0..0, "keep what");
}

#[test]
fn terminal_literal_append_cell_preserves_outside_facts_and_hard_break_safety() {
    let source = "This is the real **Rust → Dart → Flutter** editor path. Use it like an editor,\n\
not a static Markdown preview. Certified Markdown stays rendered while focused;\n\
only incomplete or temporarily pending syntax becomes exact source locally.\n";
    let mut document = DocumentSession::begin(source).expect("begin terminal append paragraph");
    pump_ready(&mut document);
    let viewport = document
        .query_viewport(1, 0..source.len(), 8)
        .expect("terminal append viewport");
    let row = &viewport.rows[0];
    let terminal = row
        .projection_edit_cells
        .iter()
        .find(|cell| cell.flags == DOCUMENT_PROJECTION_EDIT_CELL_LITERAL_APPEND_FLAGS)
        .expect("terminal append cell");
    assert_eq!(terminal.source_range, 163..238);
    assert_eq!(terminal.source_utf16_range, 159..234);
    assert_eq!(terminal.trigger_range, 238..238);
    assert_eq!(terminal.trigger_utf16_range, 234..234);
    let strong = row
        .inline_facts
        .as_ref()
        .expect("authoritative inline facts")
        .iter()
        .find(|fact| fact.kind == DocumentInlineFactKind::Strong)
        .expect("outside Strong fact")
        .clone();

    let mut edited_source = source.to_owned();
    let mut byte_end = source.len() - 1;
    for (revision, replacement) in [
        " ", "Testing", " ", "is", " ", "somewhat", " ", "useful", " ", "but", " ", "like", ".",
    ]
    .into_iter()
    .enumerate()
    {
        document
            .apply_edit(revision as u64 + 1, byte_end..byte_end, replacement)
            .expect("apply terminal literal append");
        edited_source.insert_str(byte_end, replacement);
        byte_end += replacement.len();
        pump_ready(&mut document);
        assert_current_rows_match_clean(&mut document, revision as u64 + 2, &edited_source);
        let viewport = document
            .query_viewport(revision as u64 + 2, 0..edited_source.len(), 8)
            .expect("terminal append result viewport");
        let after = viewport.rows[0]
            .inline_facts
            .as_ref()
            .expect("result facts")
            .iter()
            .find(|fact| fact.kind == DocumentInlineFactKind::Strong)
            .expect("outside Strong remains rendered");
        assert_eq!(after, &strong);
        let terminal = viewport.rows[0]
            .projection_edit_cells
            .iter()
            .find(|cell| {
                cell.flags & DOCUMENT_PROJECTION_EDIT_CELL_LITERAL_APPEND_FLAGS
                    == DOCUMENT_PROJECTION_EDIT_CELL_LITERAL_APPEND_FLAGS
            })
            .expect("fresh terminal append state");
        assert_eq!(
            terminal.flags & DOCUMENT_PROJECTION_EDIT_CELL_TERMINAL_SPACE_BLOCKED != 0,
            replacement == " ",
            "replacement={replacement:?}"
        );
    }
    document.close().expect("close terminal append paragraph");

    let mut punctuation_document =
        DocumentSession::begin(source).expect("begin terminal punctuation chain");
    pump_ready(&mut punctuation_document);
    let mut punctuation_source = source.to_owned();
    let mut punctuation_end = source.len() - 1;
    for (revision, replacement) in ["\"", "'", ",", ".", ":", ";", "?"].into_iter().enumerate() {
        punctuation_document
            .apply_edit(
                revision as u64 + 1,
                punctuation_end..punctuation_end,
                replacement,
            )
            .expect("apply bounded terminal prose punctuation");
        punctuation_source.insert_str(punctuation_end, replacement);
        punctuation_end += replacement.len();
        pump_ready(&mut punctuation_document);
        assert_current_rows_match_clean(
            &mut punctuation_document,
            revision as u64 + 2,
            &punctuation_source,
        );
        let viewport = punctuation_document
            .query_viewport(revision as u64 + 2, 0..punctuation_source.len(), 8)
            .expect("terminal punctuation result viewport");
        let after = viewport.rows[0]
            .inline_facts
            .as_ref()
            .expect("terminal punctuation facts")
            .iter()
            .find(|fact| fact.kind == DocumentInlineFactKind::Strong)
            .expect("outside Strong survives terminal punctuation");
        assert_eq!(after, &strong, "replacement={replacement:?}");
    }
    punctuation_document
        .close()
        .expect("close terminal punctuation chain");

    let no_final_newline = "Before **bold**\nplain terminal.";
    let mut document =
        DocumentSession::begin(no_final_newline).expect("begin no-final-newline paragraph");
    pump_ready(&mut document);
    let viewport = document
        .query_viewport(1, 0..no_final_newline.len(), 8)
        .expect("no-final-newline viewport");
    let eof = u64::try_from(no_final_newline.len()).expect("EOF fits u64");
    assert!(
        viewport.rows[0].projection_edit_cells.iter().any(|cell| {
            cell.flags & DOCUMENT_PROJECTION_EDIT_CELL_LITERAL_APPEND_FLAGS
                == DOCUMENT_PROJECTION_EDIT_CELL_LITERAL_APPEND_FLAGS
                && cell.trigger_range == (eof..eof)
        }),
        "plain terminal authority must include source EOF: {viewport:#?}"
    );
    document.close().expect("close no-final-newline paragraph");
    assert_single_edit_matches_clean(
        no_final_newline,
        no_final_newline.len()..no_final_newline.len(),
        " Testing.",
    );

    for structural in ["- first\n", "> first\n"] {
        let mut document = DocumentSession::begin(structural).expect("begin structural row");
        pump_ready(&mut document);
        let viewport = document
            .query_viewport(1, 0..structural.len(), 8)
            .expect("structural viewport");
        assert!(
            viewport.rows.iter().all(|row| {
                row.projection_edit_cells.iter().all(|cell| {
                    cell.flags & DOCUMENT_PROJECTION_EDIT_CELL_LITERAL_APPEND_FLAGS
                        != DOCUMENT_PROJECTION_EDIT_CELL_LITERAL_APPEND_FLAGS
                })
            }),
            "matcher 5 is a Plain-only ABI authority: {structural:?} {viewport:#?}"
        );
        document.close().expect("close structural row");
    }

    for (padded, expected_range) in [
        ("word \nnext **bold**\n", Some(0..5)),
        (" word\nnext **bold**\n", None),
    ] {
        let mut document = DocumentSession::begin(padded).expect("begin padded paragraph");
        pump_ready(&mut document);
        let viewport = document
            .query_viewport(1, 0..padded.len(), 8)
            .expect("padded paragraph viewport");
        let first = &viewport.rows[0];
        let first_line_end = u64::try_from(padded.find('\n').expect("physical line end"))
            .expect("line end fits u64");
        let first_line_cells = first
            .projection_edit_cells
            .iter()
            .filter(|cell| cell.source_range.start < first_line_end)
            .collect::<Vec<_>>();
        match expected_range {
            Some(expected_range) => {
                assert_eq!(first_line_cells.len(), 1, "{padded:?} {first:#?}");
                assert_eq!(first_line_cells[0].source_range, expected_range);
                assert_eq!(
                    first_line_cells[0].flags,
                    DOCUMENT_PROJECTION_EDIT_CELL_LITERAL_WORD_FLAGS
                );
            }
            None => assert!(first_line_cells.is_empty(), "{padded:?} {first:#?}"),
        }
        document.close().expect("close padded paragraph");
    }

    for block_opener in ["-\n", "1.\n", "#\n"] {
        let mut document = DocumentSession::begin(block_opener).expect("begin block opener");
        pump_ready(&mut document);
        let viewport = document
            .query_viewport(1, 0..block_opener.len(), 8)
            .expect("block-opener viewport");
        assert!(
            viewport.rows.iter().all(|row| {
                row.projection_edit_cells.iter().all(|cell| {
                    cell.flags & DOCUMENT_PROJECTION_EDIT_CELL_LITERAL_APPEND_FLAGS
                        != DOCUMENT_PROJECTION_EDIT_CELL_LITERAL_APPEND_FLAGS
                })
            }),
            "terminal space must not complete a block opener: {block_opener:?} {viewport:#?}"
        );
        document.close().expect("close block opener");
        let append = block_opener.len() - 1;
        assert_single_edit_matches_clean(block_opener, append..append, " ");
    }

    let absorbing_autolink_tail = "Visit www.commonmark.org/a.b.\n";
    let mut document =
        DocumentSession::begin(absorbing_autolink_tail).expect("begin absorbing autolink tail");
    pump_ready(&mut document);
    let viewport = document
        .query_viewport(1, 0..absorbing_autolink_tail.len(), 8)
        .expect("absorbing autolink viewport");
    assert!(
        viewport.rows[0].projection_edit_cells.iter().all(|cell| {
            cell.flags & DOCUMENT_PROJECTION_EDIT_CELL_LITERAL_APPEND_FLAGS
                != DOCUMENT_PROJECTION_EDIT_CELL_LITERAL_APPEND_FLAGS
        }),
        "a same-line outside fact can absorb the terminal tail: {viewport:#?}"
    );
    document.close().expect("close absorbing autolink tail");
    let append = absorbing_autolink_tail.len() - 1;
    assert_single_edit_matches_clean(absorbing_autolink_tail, append..append, "x");
}

#[test]
fn every_literal_splice_and_delete_preserves_outside_facts_differentially() {
    let source = "ab **x** cd _y_\n";
    let mut base = DocumentSession::begin(source).expect("begin base dogfood paragraph");
    pump_ready(&mut base);
    let viewport = base
        .query_viewport(1, 0..source.len(), 8)
        .expect("base dogfood paragraph viewport");
    let row = &viewport.rows[0];
    let cells = row.projection_edit_cells.clone();
    let before = row.inline_facts.as_ref().expect("base facts").clone();
    assert_eq!(before.len(), 2, "the oracle must cover every outside fact");
    base.close().expect("close base dogfood paragraph");

    for cell in cells
        .iter()
        .filter(|cell| cell.flags == DOCUMENT_PROJECTION_EDIT_CELL_LITERAL_WORD_FLAGS)
    {
        for start in cell.trigger_range.start..=cell.trigger_range.end {
            for end in start..=cell.trigger_range.end {
                for replacement in ["a", "Z", "0", "word"] {
                    let mut edited = DocumentSession::begin(source).expect("begin differential");
                    pump_ready(&mut edited);
                    let start_usize = usize::try_from(start).expect("start fits usize");
                    let end_usize = usize::try_from(end).expect("end fits usize");
                    edited
                        .apply_edit(1, start_usize..end_usize, replacement)
                        .expect("apply admitted ASCII word splice");
                    pump_ready(&mut edited);
                    let mut edited_source = source.to_owned();
                    edited_source.replace_range(start_usize..end_usize, replacement);
                    assert_current_rows_match_clean(&mut edited, 2, &edited_source);
                    let removed = usize::try_from(end - start).expect("removed length");
                    let result_len = source.len() + replacement.len() - removed;
                    let viewport = edited
                        .query_viewport(2, 0..result_len, 8)
                        .expect("edited differential viewport");
                    let after = viewport.rows[0]
                        .inline_facts
                        .as_ref()
                        .expect("edited facts");
                    let signed_delta = replacement.len() as i64 - (end - start) as i64;
                    let expected = before
                        .iter()
                        .cloned()
                        .map(|fact| {
                            let delta = if end <= fact.source_range.start {
                                signed_delta
                            } else {
                                0
                            };
                            shift_inline_fact(fact, delta)
                        })
                        .collect::<Vec<_>>();
                    assert_eq!(
                        after, &expected,
                        "the complete outside-fact set changed for range={start}..{end} replacement={replacement:?}"
                    );
                    edited.close().expect("close differential");
                }
            }
        }
    }

    for cell in cells
        .iter()
        .filter(|cell| cell.flags == DOCUMENT_PROJECTION_EDIT_CELL_LITERAL_WORD_FLAGS)
    {
        let first = cell.trigger_range.start.max(cell.source_range.start + 1);
        let last = cell.trigger_range.end.min(cell.source_range.end - 1);
        for position in first..=last {
            let mut edited = DocumentSession::begin(source).expect("begin space differential");
            pump_ready(&mut edited);
            let position_usize = usize::try_from(position).expect("space position fits usize");
            edited
                .apply_edit(1, position_usize..position_usize, " ")
                .expect("apply admitted interior space");
            pump_ready(&mut edited);
            let mut edited_source = source.to_owned();
            edited_source.insert(position_usize, ' ');
            assert_current_rows_match_clean(&mut edited, 2, &edited_source);
            let viewport = edited
                .query_viewport(2, 0..source.len() + 1, 8)
                .expect("spaced differential viewport");
            let after = viewport.rows[0]
                .inline_facts
                .as_ref()
                .expect("spaced facts");
            let expected = before
                .iter()
                .cloned()
                .map(|fact| {
                    let delta = if position <= fact.source_range.start {
                        1
                    } else {
                        0
                    };
                    shift_inline_fact(fact, delta)
                })
                .collect::<Vec<_>>();
            assert_eq!(
                after, &expected,
                "the complete outside-fact set changed at space position={position}"
            );
            edited.close().expect("close space differential");
        }
    }

    for cell in cells
        .iter()
        .filter(|cell| cell.flags == DOCUMENT_PROJECTION_EDIT_CELL_LITERAL_DELETE_ONE_FLAGS)
    {
        for start in cell.trigger_range.start..cell.trigger_range.end {
            let mut edited = DocumentSession::begin(source).expect("begin delete differential");
            pump_ready(&mut edited);
            let start_usize = usize::try_from(start).expect("delete start fits usize");
            edited
                .apply_edit(1, start_usize..start_usize + 1, "")
                .expect("apply admitted one-unit deletion");
            pump_ready(&mut edited);
            let mut edited_source = source.to_owned();
            edited_source.replace_range(start_usize..start_usize + 1, "");
            assert_current_rows_match_clean(&mut edited, 2, &edited_source);
            let viewport = edited
                .query_viewport(2, 0..source.len() - 1, 8)
                .expect("deleted differential viewport");
            let after = viewport.rows[0]
                .inline_facts
                .as_ref()
                .expect("deleted facts");
            let expected = before
                .iter()
                .cloned()
                .map(|fact| {
                    let delta = if start + 1 <= fact.source_range.start {
                        -1
                    } else {
                        0
                    };
                    shift_inline_fact(fact, delta)
                })
                .collect::<Vec<_>>();
            assert_eq!(
                after, &expected,
                "the complete outside-fact set changed at delete start={start}"
            );
            edited.close().expect("close delete differential");
        }
    }
}

#[test]
fn carried_edit_cell_successors_match_a_clean_final_parse() {
    let mut literal_source = "This is the real **Rust → Dart → Flutter** editor path.\n".to_owned();
    let mut literal = DocumentSession::begin(&literal_source).expect("begin literal chain");
    pump_ready(&mut literal);
    let edits = [(4_usize..4_usize, "word"), (4..4, " "), (0..4, "That")];
    for (index, (range, replacement)) in edits.into_iter().enumerate() {
        literal
            .apply_edit(index as u64 + 1, range.clone(), replacement)
            .expect("apply carried literal edit");
        literal_source.replace_range(range, replacement);
        pump_ready(&mut literal);
        assert_current_rows_match_clean(&mut literal, index as u64 + 2, &literal_source);
    }
    literal.close().expect("close literal chain");

    let mut heading_source = "# Heading\n".to_owned();
    let mut heading = DocumentSession::begin(&heading_source).expect("begin heading chain");
    pump_ready(&mut heading);
    let edits = [(2_usize..9_usize, "Café"), (2..2, "Live "), (7..8, "")];
    for (index, (range, replacement)) in edits.into_iter().enumerate() {
        heading
            .apply_edit(index as u64 + 1, range.clone(), replacement)
            .expect("apply carried heading edit");
        heading_source.replace_range(range, replacement);
        pump_ready(&mut heading);
        assert_current_rows_match_clean(&mut heading, index as u64 + 2, &heading_source);
    }
    heading.close().expect("close heading chain");
}

#[test]
fn simple_list_and_quote_shells_publish_literal_word_cells() {
    for source in ["- first **bold**\n", "> first **bold**\n"] {
        let mut document = DocumentSession::begin(source).expect("begin simple block shell");
        pump_ready(&mut document);
        let viewport = document
            .query_viewport(1, 0..source.len(), 8)
            .expect("simple block-shell viewport");
        let row = &viewport.rows[0];
        assert!(matches!(
            row.presentation,
            DocumentViewportRowPresentation::ListItem {
                nesting_depth: 1,
                simple_continuation: true,
                ..
            } | DocumentViewportRowPresentation::BlockQuote {
                nesting_depth: 1,
                simple_continuation: true,
                ..
            }
        ));
        assert_eq!(
            ordinary_projection_edit_cells(row),
            vec![
                DocumentProjectionEditCell {
                    source_range: 2..8,
                    source_utf16_range: 2..8,
                    trigger_range: 2..7,
                    trigger_utf16_range: 2..7,
                    flags: DOCUMENT_PROJECTION_EDIT_CELL_LITERAL_WORD_FLAGS,
                    replacement_first: 0,
                    replacement_second: 0,
                    result_block_shell: None,
                },
                DocumentProjectionEditCell {
                    source_range: 2..8,
                    source_utf16_range: 2..8,
                    trigger_range: 2..7,
                    trigger_utf16_range: 2..7,
                    flags: DOCUMENT_PROJECTION_EDIT_CELL_LITERAL_DELETE_ONE_FLAGS,
                    replacement_first: 0,
                    replacement_second: 0,
                    result_block_shell: None,
                },
                DocumentProjectionEditCell {
                    source_range: 8..16,
                    source_utf16_range: 8..16,
                    trigger_range: 10..10,
                    trigger_utf16_range: 10..10,
                    flags: DOCUMENT_PROJECTION_EDIT_CELL_STRONG_OPENING_SPACE_FLAGS,
                    replacement_first: 0,
                    replacement_second: 0,
                    result_block_shell: None,
                },
            ],
            "{source:?} {row:#?}"
        );
        let before = row
            .inline_facts
            .as_ref()
            .expect("shell facts")
            .iter()
            .find(|fact| fact.kind == DocumentInlineFactKind::Strong)
            .expect("shell Strong fact")
            .clone();
        document
            .apply_edit(1, 2..2, "x")
            .expect("edit shell literal segment");
        pump_ready(&mut document);
        let mut edited_source = source.to_owned();
        edited_source.insert(2, 'x');
        assert_current_rows_match_clean(&mut document, 2, &edited_source);
        let viewport = document
            .query_viewport(2, 0..source.len() + 1, 8)
            .expect("edited shell viewport");
        let after = viewport.rows[0]
            .inline_facts
            .as_ref()
            .expect("edited shell facts")
            .iter()
            .find(|fact| fact.kind == DocumentInlineFactKind::Strong)
            .expect("edited shell Strong fact");
        assert_eq!(
            after.source_range,
            before.source_range.start + 1..before.source_range.end + 1
        );
        assert_eq!(
            after.source_utf16_range,
            before.source_utf16_range.start + 1..before.source_utf16_range.end + 1
        );
        document.close().expect("close simple block shell");
    }
}

#[test]
fn list_quote_and_table_cell_edit_boundaries_match_clean_parses() {
    for source in ["- first **bold**\n", "> first **bold**\n"] {
        for (range, replacement) in [
            (2_usize..2_usize, "x"),
            (4..6, "word"),
            (3..4, ""),
            (4..4, " "),
        ] {
            assert_single_edit_matches_clean(source, range, replacement);
        }
    }
    let table = "| foo | **bold** |\n| --- | --- |\n";
    for (range, replacement) in [
        (2_usize..2_usize, "x"),
        (3..5, "word"),
        (4..5, ""),
        (3..3, " "),
    ] {
        assert_single_edit_matches_clean(table, range, replacement);
    }
}

#[test]
fn plain_table_cell_publishes_a_literal_word_edit_cell() {
    let source = "| foo | **bold** |\n| --- | --- |\n";
    let mut document = DocumentSession::begin(source).expect("begin simple table");
    pump_ready(&mut document);
    let viewport = document
        .query_viewport(1, 0..source.len(), 8)
        .expect("simple table viewport");
    let row = viewport
        .rows
        .iter()
        .find(|row| row.presentation == DocumentViewportRowPresentation::Table)
        .expect("table row");
    assert_eq!(
        row.projection_edit_cells,
        vec![
            DocumentProjectionEditCell {
                source_range: 2..5,
                source_utf16_range: 2..5,
                trigger_range: 2..5,
                trigger_utf16_range: 2..5,
                flags: DOCUMENT_PROJECTION_EDIT_CELL_LITERAL_WORD_FLAGS,
                replacement_first: 0,
                replacement_second: 0,
                result_block_shell: None,
            },
            DocumentProjectionEditCell {
                source_range: 2..5,
                source_utf16_range: 2..5,
                trigger_range: 2..5,
                trigger_utf16_range: 2..5,
                flags: DOCUMENT_PROJECTION_EDIT_CELL_LITERAL_DELETE_ONE_FLAGS,
                replacement_first: 0,
                replacement_second: 0,
                result_block_shell: None,
            },
        ]
    );
    let before = row
        .inline_facts
        .as_ref()
        .expect("table facts")
        .iter()
        .find(|fact| fact.kind == DocumentInlineFactKind::Strong)
        .expect("outside table Strong fact")
        .clone();
    document
        .apply_edit(1, 3..3, "x")
        .expect("edit plain table cell");
    pump_ready(&mut document);
    let mut edited_source = source.to_owned();
    edited_source.insert(3, 'x');
    assert_current_rows_match_clean(&mut document, 2, &edited_source);
    let viewport = document
        .query_viewport(2, 0..source.len() + 1, 8)
        .expect("edited table viewport");
    let row = viewport
        .rows
        .iter()
        .find(|row| row.presentation == DocumentViewportRowPresentation::Table)
        .expect("edited table row");
    let after = row
        .inline_facts
        .as_ref()
        .expect("edited table facts")
        .iter()
        .find(|fact| fact.kind == DocumentInlineFactKind::Strong)
        .expect("shifted outside table Strong fact");
    assert_eq!(
        after.source_range,
        before.source_range.start + 1..before.source_range.end + 1
    );
    assert_eq!(
        after.source_utf16_range,
        before.source_utf16_range.start + 1..before.source_utf16_range.end + 1
    );
    document.close().expect("close simple table");
}

#[test]
fn single_character_table_cell_authorizes_an_empty_result_cell() {
    let source = "| a | b |\n| --- | --- |\n";
    let mut document = DocumentSession::begin(source).expect("begin one-character table");
    pump_ready(&mut document);
    let viewport = document
        .query_viewport(1, 0..source.len(), 8)
        .expect("one-character table viewport");
    let row = viewport
        .rows
        .iter()
        .find(|row| row.presentation == DocumentViewportRowPresentation::Table)
        .expect("table row");
    assert!(row.projection_edit_cells.iter().any(|cell| {
        cell.source_range == (2..3)
            && cell.source_utf16_range == (2..3)
            && cell.flags
                == DOCUMENT_PROJECTION_EDIT_CELL_LITERAL_DELETE_ONE_FLAGS
                    | DOCUMENT_PROJECTION_EDIT_CELL_EMPTY_LITERAL_RESULT
            && cell.replacement_first == 1
            && cell.replacement_second == 0
    }));

    document
        .apply_edit(1, 2..3, "")
        .expect("empty the one-character cell");
    pump_ready(&mut document);
    assert_current_rows_match_clean(&mut document, 2, "|  | b |\n| --- | --- |\n");
    document.close().expect("close one-character table");
}

#[test]
fn burst_table_to_plain_transition_drops_stale_table_facts() {
    let source = "| a | b |\n| --- | --- |\n| c | d |\n\n**sentinel**\n";
    let mut document = DocumentSession::begin(source).expect("begin transition table");
    pump_ready(&mut document);

    document
        .apply_edit(1, 3..3, "\n")
        .expect("split the first table row");
    pump_ready(&mut document);
    document
        .apply_edit(2, 4..4, "*")
        .expect("start a block transition");
    document
        .apply_edit(3, 5..5, "\n")
        .expect("interrupt the block transition before parsing settles");
    pump_ready(&mut document);

    let expected = "| a\n*\n | b |\n| --- | --- |\n| c | d |\n\n**sentinel**\n";
    assert_current_rows_match_clean(&mut document, 4, expected);
    let viewport = document
        .query_viewport(4, 0..expected.len(), 32)
        .expect("transition result viewport");
    for row in viewport.rows {
        let table_facts = row
            .inline_facts
            .as_deref()
            .unwrap_or_default()
            .iter()
            .filter(|fact| fact.kind == DocumentInlineFactKind::TableCell)
            .count();
        assert_eq!(
            row.presentation == DocumentViewportRowPresentation::Table,
            table_facts > 0,
            "table presentation and fact authority must agree for {row:#?}"
        );
    }
    document.close().expect("close transition table");
}

#[test]
fn paragraph_break_then_delimiters_publish_nested_list_table_facts() {
    let source = "| a | b |\n| --- | --- |\n| c | d |\n\n**sentinel**\n";
    let mut document = DocumentSession::begin(source).expect("begin delimiter transition table");
    pump_ready(&mut document);

    document
        .apply_edit(1, 3..3, "\n")
        .expect("split the first table row");
    document
        .apply_edit(2, 4..4, "*")
        .expect("start delimiter row while parser is pending");
    pump_ready(&mut document);
    document
        .apply_edit(3, 6..6, "]")
        .expect("extend delimiter row after the first result settles");
    pump_ready(&mut document);

    let expected = "| a\n* ]| b |\n| --- | --- |\n| c | d |\n\n**sentinel**\n";
    assert_current_rows_match_clean(&mut document, 4, expected);
    let viewport = document
        .query_viewport(4, 0..expected.len(), 32)
        .expect("delimiter transition viewport");
    let nested = viewport
        .rows
        .iter()
        .find(|row| {
            matches!(
                row.presentation,
                DocumentViewportRowPresentation::ListItem { .. }
            )
        })
        .expect("nested list/table row");
    assert!(nested
        .inline_facts
        .as_deref()
        .unwrap_or_default()
        .iter()
        .any(|fact| fact.kind == DocumentInlineFactKind::TableCell));
    document.close().expect("close delimiter transition table");
}

#[test]
fn literal_word_cells_fail_closed_for_lexical_and_dependency_boundaries() {
    for source in [
        "&am; **bold** tail\n",
        "plain-text **bold** tail\n",
        "Café **bold** tail\n",
        "**bold** _right_\n",
    ] {
        let mut document = DocumentSession::begin(source).expect("begin rejected literal gap");
        pump_ready(&mut document);
        let viewport = document
            .query_viewport(1, 0..source.len(), 8)
            .expect("rejected literal gap viewport");
        let cells = viewport.rows[0]
            .projection_edit_cells
            .iter()
            .filter(|cell| {
                matches!(
                    cell.flags,
                    DOCUMENT_PROJECTION_EDIT_CELL_LITERAL_WORD_FLAGS
                        | DOCUMENT_PROJECTION_EDIT_CELL_LITERAL_DELETE_ONE_FLAGS
                )
            })
            .collect::<Vec<_>>();
        assert!(
            cells.iter().all(|cell| {
                let affected_start =
                    usize::try_from(cell.source_range.start).expect("affected start");
                let affected_end =
                    usize::try_from(cell.source_range.end).expect("affected end");
                let trigger_start =
                    usize::try_from(cell.trigger_range.start).expect("trigger start");
                let trigger_end = usize::try_from(cell.trigger_range.end).expect("trigger end");
                let affected = &source[affected_start..affected_end];
                let trigger = &source[trigger_start..trigger_end];
                let trigger_is_ascii_prose = trigger
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b' ');
                let affected_has_punctuation = affected
                    .bytes()
                    .any(|byte| !byte.is_ascii_alphanumeric() && byte != b' ');
                trigger_is_ascii_prose
                    && (!affected_has_punctuation
                        || (cell.flags == DOCUMENT_PROJECTION_EDIT_CELL_LITERAL_WORD_FLAGS
                            && cell.source_range.start < cell.trigger_range.start
                            && cell.trigger_range.end < cell.source_range.end))
            }),
            "lexical punctuation may be exact closure but never edit authority: {source:?} {cells:#?}"
        );
        for cell in cells {
            if cell.source_range.end < viewport.rows[0].editable_range.as_ref().unwrap().end {
                assert!(
                    cell.trigger_range.end < cell.source_range.end,
                    "a following dependency boundary must remain excluded: {source:?} {cell:#?}"
                );
            }
            if cell.source_range.start > viewport.rows[0].editable_range.as_ref().unwrap().start {
                let prior = usize::try_from(cell.source_range.start - 1).expect("prior source");
                assert!(
                    source.as_bytes()[prior].is_ascii_whitespace()
                        || cell.trigger_range.start > cell.source_range.start,
                    "a preceding dependency boundary must remain excluded: {source:?} {cell:#?}"
                );
            }
        }
        document.close().expect("close rejected literal gap");
    }
}

#[test]
fn flat_strong_opening_space_cell_rejects_ambiguous_dependencies_and_shapes() {
    for source in [
        "# **left** *middle* _right_\n",
        "# ***left*** _right_\n",
        "# __left__ _right_\n",
        "# **left right** _right_\n",
        "# **léft** _right_\n",
    ] {
        let mut document = DocumentSession::begin(source).expect("begin rejected Strong shape");
        pump_ready(&mut document);
        let viewport = document
            .query_viewport(1, 0..source.len(), 8)
            .expect("rejected Strong shape viewport");
        assert!(
            viewport
                .rows
                .iter()
                .all(|row| ordinary_projection_edit_cells(row).is_empty()),
            "ambiguous dependencies and noncanonical shapes fail closed: {source:?} {:#?}",
            viewport.rows,
        );
        document.close().expect("close rejected Strong shape");
    }
}

#[test]
fn plain_atx_projection_edit_cells_fail_closed_around_unsupported_rows() {
    let source = "# Test is here\n";
    let mut eligible = DocumentSession::begin(source).expect("begin eligible ATX heading");
    pump_ready(&mut eligible);
    let viewport = eligible
        .query_viewport(1, 0..source.len(), 8)
        .expect("eligible ATX heading viewport");
    let row = &viewport.rows[0];
    assert_eq!(
        row.presentation,
        DocumentViewportRowPresentation::Heading {
            level: 1,
            style: DocumentHeadingStyle::Atx,
        }
    );
    assert!(
        row.inline_facts
            .as_ref()
            .is_some_and(|facts| facts.is_empty()),
        "the parser must authoritatively prove the heading has no inline facts"
    );
    assert_eq!(row.editable_range, Some(2..14));
    assert_eq!(row.editable_utf16_range, Some(2..14));
    assert_eq!(
        ordinary_projection_edit_cells(row),
        vec![DocumentProjectionEditCell {
            source_range: 2..14,
            source_utf16_range: 2..14,
            trigger_range: 2..14,
            trigger_utf16_range: 2..14,
            flags: DOCUMENT_PROJECTION_EDIT_CELL_PLAIN_ATX_FLAGS,
            replacement_first: 0,
            replacement_second: 0,
            result_block_shell: None,
        }]
    );
    assert!(row.literal_safe_envelopes.is_empty());
    eligible.close().expect("close eligible ATX heading");

    let punctuation_source = "# Test-is here\n";
    let mut punctuation =
        DocumentSession::begin(punctuation_source).expect("begin punctuated ATX heading");
    pump_ready(&mut punctuation);
    let viewport = punctuation
        .query_viewport(1, 0..punctuation_source.len(), 8)
        .expect("punctuated ATX heading viewport");
    let row = &viewport.rows[0];
    assert!(matches!(
        row.presentation,
        DocumentViewportRowPresentation::Heading {
            style: DocumentHeadingStyle::Atx,
            ..
        }
    ));
    assert!(row
        .inline_facts
        .as_ref()
        .is_some_and(|facts| facts.is_empty()));
    assert_eq!(ordinary_projection_edit_cells(row).len(), 1);
    assert!(row.literal_safe_envelopes.is_empty());
    punctuation.close().expect("close punctuated ATX heading");

    for whitespace_source in ["#  Test is here\n", "# Test is here \n"] {
        let mut whitespace = DocumentSession::begin(whitespace_source)
            .expect("begin whitespace-boundary ATX heading");
        pump_ready(&mut whitespace);
        let viewport = whitespace
            .query_viewport(1, 0..whitespace_source.len(), 8)
            .expect("whitespace-boundary ATX heading viewport");
        assert!(
            ordinary_projection_edit_cells(&viewport.rows[0]).is_empty(),
            "leading/trailing whitespace normalization must fail closed: {whitespace_source:?}"
        );
        whitespace
            .close()
            .expect("close whitespace-boundary ATX heading");
    }

    let bom_source = "\u{feff}# Heading\n";
    let mut bom = DocumentSession::begin(bom_source).expect("begin BOF BOM heading");
    pump_ready(&mut bom);
    let viewport = bom
        .query_viewport(1, 0..bom_source.len(), 8)
        .expect("BOF BOM heading viewport");
    assert!(
        viewport
            .rows
            .iter()
            .all(|row| ordinary_projection_edit_cells(row).is_empty()),
        "the first contract requires an exact canonical ATX prefix"
    );
    bom.close().expect("close BOF BOM heading");

    let inline_source = "# Test *is* here\n";
    let mut inline = DocumentSession::begin(inline_source).expect("begin inline ATX heading");
    pump_ready(&mut inline);
    let viewport = inline
        .query_viewport(1, 0..inline_source.len(), 8)
        .expect("inline ATX heading viewport");
    let row = &viewport.rows[0];
    assert!(matches!(
        row.presentation,
        DocumentViewportRowPresentation::Heading {
            style: DocumentHeadingStyle::Atx,
            ..
        }
    ));
    assert!(
        row.inline_facts
            .as_ref()
            .is_some_and(|facts| !facts.is_empty()),
        "the inline hazard must be parser-authored"
    );
    assert!(
        ordinary_projection_edit_cells(row).is_empty(),
        "the first edit-cell contract cannot retain inline facts"
    );
    assert_eq!(
        row.literal_safe_envelopes,
        vec![
            flark_runtime::DocumentLiteralSafeEnvelope {
                edit_class: DocumentLiteralEditClass::AsciiWordInsertion,
                source_range: 8..10,
                source_utf16_range: 8..10,
            },
            flark_runtime::DocumentLiteralSafeEnvelope {
                edit_class: DocumentLiteralEditClass::SingleAsciiLiteralUnitDeletion,
                source_range: 8..10,
                source_utf16_range: 8..10,
            },
        ],
        "inline facts may retain narrow authority, never whole-heading authority"
    );
    inline.close().expect("close inline ATX heading");

    let setext_source = "Test is here\n============\n";
    let mut setext = DocumentSession::begin(setext_source).expect("begin Setext heading");
    pump_ready(&mut setext);
    let viewport = setext
        .query_viewport(1, 0..setext_source.len(), 8)
        .expect("Setext heading viewport");
    let row = &viewport.rows[0];
    assert!(matches!(
        row.presentation,
        DocumentViewportRowPresentation::Heading {
            style: DocumentHeadingStyle::Setext,
            ..
        }
    ));
    assert!(row
        .inline_facts
        .as_ref()
        .is_some_and(|facts| facts.is_empty()));
    assert!(
        ordinary_projection_edit_cells(row).is_empty(),
        "plain Setext content must not receive an ATX edit cell"
    );
    setext.close().expect("close Setext heading");

    for nested_source in ["> # Nested\n", "> # \n", "- # Nested\n", "- # \n"] {
        let mut nested = DocumentSession::begin(nested_source).expect("begin nested ATX heading");
        pump_ready(&mut nested);
        let viewport = nested
            .query_viewport(1, 0..nested_source.len(), 8)
            .expect("nested ATX heading viewport");
        assert!(
            viewport
                .rows
                .iter()
                .all(|row| ordinary_projection_edit_cells(row).is_empty()),
            "the first edit-cell contract is top-level only: {nested_source:?} {:#?}",
            viewport.rows,
        );
        nested.close().expect("close nested ATX heading");
    }

    let oversized_source = format!("# {}\n", "a".repeat(4 * 1024));
    let mut oversized =
        DocumentSession::begin(&oversized_source).expect("begin oversized ATX heading");
    pump_ready(&mut oversized);
    let viewport = oversized
        .query_viewport(1, 0..oversized_source.len(), 8)
        .expect("oversized ATX heading viewport");
    assert!(
        viewport
            .rows
            .iter()
            .all(|row| ordinary_projection_edit_cells(row).is_empty()),
        "rows beyond the simple-line bound must fail closed"
    );
    oversized.close().expect("close oversized ATX heading");
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

#[test]
fn nonzero_multibyte_viewport_rows_keep_global_utf16_coordinates() {
    let initial = "Head.\n\nTrailing.\n";
    let mut document = DocumentSession::begin(initial).expect("begin multibyte document");
    pump_ready(&mut document);
    let mut inserted = String::new();
    for chunk in 0..5 {
        let paragraphs = (0..9)
            .map(|index| format!("{} {chunk}-{index}", "😀".repeat(100)))
            .collect::<Vec<_>>()
            .join("\n\n");
        let structured_tail = if chunk == 4 {
            "Active.\n\nalpha123\n\n> **bold** and `code`\n> tail\n\n- [x] task with *emphasis*\n\n| α | β |\n| :--- | ---: |\n| *x* | [y](https://example.test) |\n"
        } else {
            ""
        };
        let addition = format!("{paragraphs}\n\n{structured_tail}");
        let insertion_offset = 5 + inserted.len();
        document
            .apply_edit(
                document.revision(),
                insertion_offset..insertion_offset,
                &addition,
            )
            .expect("insert multibyte viewport chunk");
        inserted.push_str(&addition);
    }
    pump_ready(&mut document);
    let source = format!("Head.{inserted}\n\nTrailing.\n");
    let active = source.find("Active.").expect("active row");

    let viewport = document
        .query_viewport(document.revision(), active..source.len(), 32)
        .expect("query nonzero multibyte viewport");
    assert!(!viewport.rows.is_empty());
    let active_utf16 = source[..active].encode_utf16().count() as u64;
    let mut saw_inline_fact = false;
    let mut saw_literal_safe_envelope = false;
    let mut saw_list_prefix = false;
    let mut saw_quote_prefix = false;
    let mut saw_projection_segments = false;
    let mut saw_table_metadata = false;
    for row in &viewport.rows {
        assert!(row.source_utf16_range.start >= active_utf16);
        assert_eq!(
            row.source_utf16_range,
            expected_utf16_range(&source, &row.source_range),
            "row byte and UTF-16 ranges must share global source coordinates"
        );
        if let (Some(bytes), Some(utf16)) = (&row.editable_range, &row.editable_utf16_range) {
            assert_eq!(
                *utf16,
                expected_utf16_range(&source, bytes),
                "editable byte and UTF-16 ranges must share global source coordinates"
            );
        } else {
            assert_eq!(
                row.editable_range.is_some(),
                row.editable_utf16_range.is_some()
            );
        }
        match row.presentation {
            DocumentViewportRowPresentation::ListItem {
                prefix_start_byte,
                prefix_end_byte,
                prefix_start_utf16,
                prefix_end_utf16,
                item_end_byte,
                item_end_utf16,
                ..
            } => {
                assert_eq!(
                    prefix_start_utf16..prefix_end_utf16,
                    expected_utf16_range(&source, &(prefix_start_byte..prefix_end_byte))
                );
                assert_eq!(
                    item_end_utf16,
                    expected_utf16_range(&source, &(item_end_byte..item_end_byte)).start
                );
                saw_list_prefix = true;
            }
            DocumentViewportRowPresentation::BlockQuote {
                prefix_start_byte,
                prefix_end_byte,
                prefix_start_utf16,
                prefix_end_utf16,
                ..
            } => {
                assert_eq!(
                    prefix_start_utf16..prefix_end_utf16,
                    expected_utf16_range(&source, &(prefix_start_byte..prefix_end_byte))
                );
                saw_quote_prefix = true;
            }
            DocumentViewportRowPresentation::Table => saw_table_metadata = true,
            _ => {}
        }
        for fact in row.inline_facts.iter().flatten() {
            assert_eq!(
                fact.source_utf16_range,
                expected_utf16_range(&source, &fact.source_range)
            );
            assert_eq!(
                fact.content_utf16_range,
                expected_utf16_range(&source, &fact.content_range)
            );
            saw_inline_fact = true;
            if fact.kind == DocumentInlineFactKind::TableCell {
                saw_table_metadata = true;
            }
        }
        for envelope in &row.literal_safe_envelopes {
            assert_eq!(
                envelope.source_utf16_range,
                expected_utf16_range(&source, &envelope.source_range)
            );
            saw_literal_safe_envelope = true;
        }
        for segment in row.projection_segments.iter().flatten() {
            assert_eq!(
                segment.source_utf16_range,
                expected_utf16_range(&source, &segment.source_range)
            );
            saw_projection_segments = true;
        }
    }
    assert!(
        saw_inline_fact,
        "fixture must exercise inline fact geometry"
    );
    assert!(
        saw_literal_safe_envelope,
        "fixture must exercise literal-safe envelope geometry"
    );
    assert!(
        saw_list_prefix,
        "fixture must exercise List prefix geometry"
    );
    assert!(
        saw_quote_prefix,
        "fixture must exercise quote prefix geometry"
    );
    assert!(
        saw_projection_segments,
        "fixture must exercise projected-segment geometry"
    );
    assert!(
        saw_table_metadata,
        "fixture must exercise table-cell geometry"
    );
    document.close().expect("close document");
}
