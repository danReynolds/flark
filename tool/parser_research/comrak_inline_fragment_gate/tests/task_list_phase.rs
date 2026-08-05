use std::ops::Range;
use std::path::Path;

use comrak::inline_fragment::{
    EMPTY_REFERENCE_SNAPSHOT, INLINE_FACT_FLAG_TASK_CHECKED, InlineFactKind, InlineFragment,
    InlineFragmentRequest, InlineInputKind, InlineProfile, InlineProjectionFactKind,
    InlineReferenceSnapshot, InlineReferenceTarget, parse_inline_fragment,
};
use comrak::nodes::{NodeValue, Sourcepos};
use comrak::{Arena, Options, markdown_to_html, parse_document};
use flark_comrak_inline_fragment_gate::{LogicalOriginMap, LogicalOriginRun, OriginRunKind};

#[derive(Clone, Debug, PartialEq, Eq)]
struct TaskReceipt {
    checked: bool,
    symbol: Range<usize>,
    marker: Range<usize>,
}

fn gfm_options() -> Options<'static> {
    let mut options = Options::default();
    options.extension.autolink = true;
    options.extension.strikethrough = true;
    options.extension.table = true;
    options.extension.tagfilter = true;
    options.extension.tasklist = true;
    assert!(!options.parse.relaxed_tasklist_matching);
    assert!(!options.parse.tasklist_in_table);
    options
}

fn parse_leaf(logical: &str, kind: InlineInputKind, profile: InlineProfile) -> InlineFragment {
    parse_inline_fragment(InlineFragmentRequest {
        logical,
        leaf_id: 41,
        kind,
        profile,
        reference_snapshot: &EMPTY_REFERENCE_SNAPSHOT,
        revision: 9,
        expected_revision: 9,
    })
    .unwrap()
}

fn leaf_task(fragment: &InlineFragment) -> Option<TaskReceipt> {
    let task = fragment
        .facts
        .iter()
        .find(|fact| fact.kind == InlineFactKind::TaskListMarker as u8)?;
    let symbol = task.logical_start as usize..(task.logical_start + task.logical_len) as usize;
    let marker = fragment
        .projection_facts
        .iter()
        .find(|fact| {
            if fact.kind != InlineProjectionFactKind::HiddenMarker as u8 {
                return false;
            }
            let start = fact.logical_start as usize;
            let end = (fact.logical_start + fact.logical_len) as usize;
            start <= symbol.start && end >= symbol.end
        })
        .expect("task fact has a complete hidden marker projection");
    Some(TaskReceipt {
        checked: task.flags & INLINE_FACT_FLAG_TASK_CHECKED != 0,
        symbol,
        marker: marker.logical_start as usize..(marker.logical_start + marker.logical_len) as usize,
    })
}

fn line_starts(source: &str) -> Vec<usize> {
    let mut starts = vec![0];
    for (index, byte) in source.bytes().enumerate() {
        if byte == b'\n' {
            starts.push(index + 1);
        }
    }
    starts
}

fn byte_range(source: &str, sourcepos: Sourcepos) -> Range<usize> {
    let starts = line_starts(source);
    let start = starts[sourcepos.start.line - 1] + sourcepos.start.column - 1;
    let end = starts[sourcepos.end.line - 1] + sourcepos.end.column;
    start..end
}

fn full_tasks(source: &str) -> Vec<(bool, Range<usize>)> {
    let arena = Arena::new();
    let options = gfm_options();
    let root = parse_document(&arena, source, &options);
    root.descendants()
        .filter_map(|node| {
            let ast = node.data();
            let NodeValue::TaskItem(task) = ast.value else {
                return None;
            };
            Some((
                task.symbol.is_some(),
                byte_range(source, task.symbol_sourcepos),
            ))
        })
        .collect()
}

fn count(source: &str, needle: &str) -> usize {
    source.match_indices(needle).count()
}

#[derive(Debug)]
struct DefinedX;

impl InlineReferenceSnapshot for DefinedX {
    fn identity(&self) -> u64 {
        1
    }

    fn generation(&self) -> u64 {
        1
    }

    fn resolve(&self, normalized: &str, _original: &str) -> InlineReferenceTarget {
        InlineReferenceTarget {
            symbol_id: 7,
            presence_generation: 1,
            defined: normalized == "x",
        }
    }
}

#[test]
fn strict_task_recognition_is_owned_by_comrak_and_context_gated() {
    let recognized = [
        ("[ ] todo", false, 0..4, 1..2),
        ("[x] done", true, 0..4, 1..2),
        ("[X]\tTabbed", true, 0..4, 1..2),
        (" \t[x] spaced", true, 0..6, 3..4),
        ("[x]", true, 0..3, 1..2),
        ("[ ]", false, 0..3, 1..2),
    ];
    for (logical, checked, marker, symbol) in recognized {
        let fragment = parse_leaf(
            logical,
            InlineInputKind::ListItemParagraph,
            InlineProfile::Gfm,
        );
        assert_eq!(
            leaf_task(&fragment),
            Some(TaskReceipt {
                checked,
                symbol,
                marker,
            }),
            "{logical:?}",
        );
    }

    for logical in [
        "[",
        "[ ",
        "[x",
        "[x]done",
        "[ ]todo",
        "[!] nope",
        "[あ] nope",
        "[xx] nope",
        "\\[x] escaped",
    ] {
        let fragment = parse_leaf(
            logical,
            InlineInputKind::ListItemParagraph,
            InlineProfile::Gfm,
        );
        assert_eq!(leaf_task(&fragment), None, "{logical:?}");
    }

    for (kind, profile) in [
        (InlineInputKind::Paragraph, InlineProfile::Gfm),
        (InlineInputKind::TableCell, InlineProfile::Gfm),
        (
            InlineInputKind::ListItemParagraph,
            InlineProfile::CommonMark,
        ),
    ] {
        assert_eq!(leaf_task(&parse_leaf("[x] done", kind, profile)), None);
    }
}

#[test]
fn unmatched_task_context_preserves_adjacent_text_exactly() {
    for logical in ["`one", "two`", "a_b", "left__right"] {
        assert_eq!(
            parse_leaf(
                logical,
                InlineInputKind::ListItemParagraph,
                InlineProfile::Gfm,
            ),
            parse_leaf(logical, InlineInputKind::Paragraph, InlineProfile::Gfm),
            "{logical:?}",
        );
    }
}

#[test]
fn nested_and_mixed_items_match_full_structure_html_and_symbol_origins() {
    let source = concat!(
        "- [x] outer **bold**\n",
        "- plain\n",
        "- [ ] parent\n",
        "  - [X] nested `code`\n",
        "  - plain nested\n",
    );
    let leaves = [
        ("[x] outer **bold**\n", true),
        ("plain\n", false),
        ("[ ] parent\n", true),
        ("[X] nested `code`\n", true),
        ("plain nested\n", false),
    ];

    let mut facade_tasks = Vec::new();
    let mut search_from = 0;
    for (logical, is_task) in leaves {
        let relative = source[search_from..].find(logical).unwrap();
        let physical_start = search_from + relative;
        search_from = physical_start + logical.len();
        let fragment = parse_leaf(
            logical,
            InlineInputKind::ListItemParagraph,
            InlineProfile::Gfm,
        );
        let task = leaf_task(&fragment);
        assert_eq!(task.is_some(), is_task, "{logical:?}");
        if let Some(task) = task {
            facade_tasks.push((
                task.checked,
                physical_start + task.symbol.start..physical_start + task.symbol.end,
            ));
        }
    }
    assert_eq!(facade_tasks, full_tasks(source));

    let arena = Arena::new();
    let options = gfm_options();
    let root = parse_document(&arena, source, &options);
    let task_items = root
        .descendants()
        .filter(|node| matches!(node.data().value, NodeValue::TaskItem(_)))
        .count();
    let plain_items = root
        .descendants()
        .filter(|node| matches!(node.data().value, NodeValue::Item(_)))
        .count();
    let task_lists = root
        .descendants()
        .filter(|node| matches!(node.data().value, NodeValue::List(list) if list.is_task_list))
        .count();
    assert_eq!((task_items, plain_items, task_lists), (3, 2, 2));

    let html = markdown_to_html(source, &options);
    assert_eq!(count(&html, "type=\"checkbox\""), 3);
    assert_eq!(count(&html, "checked=\"\""), 2);
    assert!(html.contains("outer <strong>bold</strong>"));
    assert!(html.contains("nested <code>code</code>"));
}

#[test]
fn multiline_crlf_unicode_and_prefix_gaps_compose_to_exact_physical_bytes() {
    let source = concat!("> - [x] first\r\n", ">   continuation 𐀀 **bold**\r\n",);
    let first = "[x] first\r\n";
    let second = "continuation 𐀀 **bold**\r\n";
    let logical = format!("{first}{second}");
    let first_physical = source.find(first).unwrap();
    let second_physical = source.find(second).unwrap();
    let split = first.len() as u32;
    let fragment = parse_leaf(
        &logical,
        InlineInputKind::ListItemParagraph,
        InlineProfile::Gfm,
    );
    let task = leaf_task(&fragment).unwrap();
    assert!(task.checked);

    let map = LogicalOriginMap {
        leaf_id: 41,
        revision: 9,
        logical_len: logical.len() as u32,
        runs: vec![
            LogicalOriginRun {
                logical: 0..split,
                physical: first_physical as u64..(first_physical + first.len()) as u64,
                kind: OriginRunKind::Identity,
            },
            LogicalOriginRun {
                logical: split..logical.len() as u32,
                physical: second_physical as u64..(second_physical + second.len()) as u64,
                kind: OriginRunKind::Identity,
            },
        ],
    };
    let composed = map.compose(&fragment).unwrap();
    let mapped_task = composed
        .semantic_facts
        .iter()
        .find(|fact| fact.fact.kind == InlineFactKind::TaskListMarker as u8)
        .unwrap();
    assert_eq!(
        mapped_task.physical_parts,
        vec![first_physical as u64 + 1..first_physical as u64 + 2]
    );
    let mapped_marker = composed
        .projection_facts
        .iter()
        .find(|fact| {
            fact.fact.kind == InlineProjectionFactKind::HiddenMarker as u8
                && fact.fact.logical_start == 0
        })
        .unwrap();
    assert_eq!(
        mapped_marker.physical_parts,
        vec![first_physical as u64..first_physical as u64 + 4]
    );
    assert_eq!(
        full_tasks(source)[0].1,
        first_physical + 1..first_physical + 2
    );

    let html = markdown_to_html(source, &gfm_options());
    assert!(html.contains("type=\"checkbox\" checked=\"\""));
    assert!(html.contains("continuation 𐀀 <strong>bold</strong>"));
    for part in composed
        .semantic_facts
        .iter()
        .chain(&composed.projection_facts)
        .flat_map(|fact| &fact.physical_parts)
    {
        assert!(!part.contains(&0));
        assert!(
            !(part.start < second_physical as u64
                && part.end > first_physical as u64 + first.len() as u64)
        );
    }
}

#[test]
fn tabs_and_incomplete_typing_follow_the_strict_full_parser_transition() {
    let tabbed = "-\t[x]\tTabbed 𐀀\r\n";
    let logical = "[x]\tTabbed 𐀀\r\n";
    let physical_start = tabbed.find(logical).unwrap();
    let fragment = parse_leaf(
        logical,
        InlineInputKind::ListItemParagraph,
        InlineProfile::Gfm,
    );
    let task = leaf_task(&fragment).unwrap();
    assert_eq!(task.symbol, 1..2);
    assert_eq!(
        physical_start + task.symbol.start..physical_start + task.symbol.end,
        tabbed.find('x').unwrap()..tabbed.find('x').unwrap() + 1,
    );
    assert_eq!(full_tasks(tabbed).len(), 1);
    assert!(markdown_to_html(tabbed, &gfm_options()).contains("checkbox"));

    let transitions = [
        ("[", false),
        ("[ ", false),
        ("[x", false),
        ("[x]", true),
        ("[ ]", true),
        ("[x]d", false),
        ("[x] ", true),
        ("[!] ", false),
        ("[あ] ", false),
    ];
    for (logical, expected) in transitions {
        let source = format!("- {logical}");
        let full = !full_tasks(&source).is_empty();
        let facade = leaf_task(&parse_leaf(
            logical,
            InlineInputKind::ListItemParagraph,
            InlineProfile::Gfm,
        ))
        .is_some();
        assert_eq!(full, expected, "full parser: {logical:?}");
        assert_eq!(facade, expected, "facade: {logical:?}");
    }
}

#[test]
fn parser_text_scanning_preserves_entity_whitespace_and_exact_source_origins() {
    let cases = [
        ("[ ]&NewLine;", false, 0..12, 1..2),
        ("[x]&Tab;done", true, 0..8, 1..2),
        ("[&#x78;] done", true, 0..9, 1..7),
        ("&Tab;[x] done", true, 0..9, 6..7),
    ];
    for (logical, checked, marker, symbol) in cases {
        let source = format!("- {logical}");
        let fragment = parse_leaf(
            logical,
            InlineInputKind::ListItemParagraph,
            InlineProfile::Gfm,
        );
        assert_eq!(
            leaf_task(&fragment),
            Some(TaskReceipt {
                checked,
                symbol,
                marker,
            }),
            "{logical:?}",
        );
        assert_eq!(full_tasks(&source).len(), 1, "{logical:?}");
        assert!(markdown_to_html(&source, &gfm_options()).contains("checkbox"));
    }

    // Both immediately-after-`]` bytes are `&`, not raw whitespace. A raw
    // scanner cannot classify these, while Comrak's post-inline scan sees the
    // decoded newline/tab and does. The entity-as-symbol case also proves the
    // fact maps the complete physical entity rather than fabricating one byte.
    assert_eq!("[ ]&NewLine;".as_bytes()[3], b'&');
    assert_eq!("[x]&Tab;done".as_bytes()[3], b'&');
}

#[test]
fn reference_presence_keeps_the_same_cross_phase_precedence_as_full_comrak() {
    let source = "- [x] done\n\n[x]: /destination\n";
    let full_is_task = !full_tasks(source).is_empty();
    let facade = parse_inline_fragment(InlineFragmentRequest {
        logical: "[x] done\n",
        leaf_id: 41,
        kind: InlineInputKind::ListItemParagraph,
        profile: InlineProfile::Gfm,
        reference_snapshot: &DefinedX,
        revision: 9,
        expected_revision: 9,
    })
    .unwrap();
    assert!(!full_is_task);
    assert_eq!(leaf_task(&facade), None);
    assert!(
        facade
            .facts
            .iter()
            .any(|fact| fact.kind == InlineFactKind::Link as u8)
    );
    assert!(
        facade
            .reference_dependencies
            .iter()
            .any(|dependency| dependency.normalized_label == "x" && dependency.resolved)
    );
}

#[test]
fn flark_selected_gfm_fixture_runs_through_the_same_strict_gate() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../test/fixtures/commonmark/gfm_cases.json");
    let fixtures: serde_json::Value =
        serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let fixture = fixtures
        .as_array()
        .unwrap()
        .iter()
        .find(|fixture| fixture["id"] == "task_list_extension")
        .unwrap();
    let markdown = fixture["markdown"].as_str().unwrap();
    let full = full_tasks(markdown);
    assert_eq!(
        full.iter().map(|(checked, _)| *checked).collect::<Vec<_>>(),
        vec![true, false]
    );

    let facade = markdown
        .lines()
        .map(|line| {
            let logical = line.strip_prefix("- ").unwrap();
            leaf_task(&parse_leaf(
                logical,
                InlineInputKind::ListItemParagraph,
                InlineProfile::Gfm,
            ))
            .unwrap()
            .checked
        })
        .collect::<Vec<_>>();
    assert_eq!(facade, vec![true, false]);

    let html = markdown_to_html(markdown, &gfm_options());
    for expected in fixture["expectedContains"].as_array().unwrap() {
        assert!(html.contains(expected.as_str().unwrap()));
    }
}
