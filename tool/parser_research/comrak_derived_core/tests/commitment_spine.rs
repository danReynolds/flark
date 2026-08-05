use std::collections::BTreeMap;

use comrak::nodes::{NodeValue, TableAlignment};
use comrak::{parse_document, Arena, Options};
use flark_comrak_derived_core_probe::commitment_spine::{
    Alignment, CommitmentFact, CommitmentSpine, CommitmentStatus, OpaqueReason,
};

fn run(source: &str, fuel: usize) -> CommitmentSpine {
    let mut machine = CommitmentSpine::new(source);
    while !machine.is_complete() {
        let report = machine.advance(fuel);
        assert!(report.work_units <= fuel);
        assert!(report.source_bytes_inspected <= report.work_units);
    }
    // `is_complete` becomes true at the line boundary before finalizers run;
    // one zero-work poll publishes document-final facts.
    machine.advance(fuel);
    machine
}

fn options() -> Options<'static> {
    let mut options = Options::default();
    options.extension.table = true;
    options.render.r#unsafe = true;
    options
}

fn stock_list_tightness(source: &str) -> Vec<bool> {
    let arena = Arena::new();
    let root = parse_document(&arena, source, &options());
    root.descendants()
        .filter_map(|node| match &node.data().value {
            NodeValue::List(list) => Some(list.tight),
            _ => None,
        })
        .collect()
}

fn gate_list_tightness(machine: &CommitmentSpine) -> Vec<bool> {
    let mut lists = machine
        .facts()
        .iter()
        .filter_map(|fact| match fact {
            CommitmentFact::List {
                source,
                depth,
                tight,
                ..
            } => Some((source.start, *depth, *tight)),
            _ => None,
        })
        .collect::<Vec<_>>();
    lists.sort_by_key(|(start, depth, _)| (*start, *depth));
    lists.into_iter().map(|(_, _, tight)| tight).collect()
}

#[test]
fn closed_prefix_list_tightness_matches_curated_comrak_matrix() {
    let fixtures = [
        "- one\n- two\n",
        "- one\n\n- two\n",
        "- one\n\n  continuation\n- two\n",
        "- outer\n  - inner\n  - tail\n- done\n",
        "- outer\n\n  - inner\n\n  - tail\n- done\n",
        "1. one\n2. two\n\nparagraph\n",
        "- a\n\n  second block\n",
    ];
    for source in fixtures {
        let machine = run(source, 7);
        assert_eq!(
            gate_list_tightness(&machine),
            stock_list_tightness(source),
            "source={source:?}\nfacts={:#?}",
            machine.facts()
        );
    }
}

#[derive(Debug, PartialEq, Eq)]
struct StockTable {
    alignments: Vec<Alignment>,
    rows: usize,
    columns: usize,
}

fn stock_tables(source: &str) -> Vec<StockTable> {
    let arena = Arena::new();
    let root = parse_document(&arena, source, &options());
    root.descendants()
        .filter_map(|node| match &node.data().value {
            NodeValue::Table(table) => Some(StockTable {
                alignments: table
                    .alignments
                    .iter()
                    .map(|alignment| match alignment {
                        TableAlignment::None => Alignment::None,
                        TableAlignment::Left => Alignment::Left,
                        TableAlignment::Center => Alignment::Center,
                        TableAlignment::Right => Alignment::Right,
                    })
                    .collect(),
                rows: table.num_rows,
                columns: table.num_columns,
            }),
            _ => None,
        })
        .collect()
}

fn gate_tables(machine: &CommitmentSpine) -> Vec<StockTable> {
    let mut tables = BTreeMap::<u64, StockTable>::new();
    for fact in machine.facts() {
        match fact {
            CommitmentFact::TableStart {
                id,
                alignments,
                header,
                ..
            } => {
                tables.insert(
                    *id,
                    StockTable {
                        alignments: alignments.to_vec(),
                        rows: 1,
                        columns: header.len(),
                    },
                );
            }
            CommitmentFact::TableRow { table_id, .. } => {
                tables.get_mut(table_id).unwrap().rows += 1
            }
            _ => {}
        }
    }
    tables.into_values().collect()
}

#[test]
fn gfm_table_activation_alignment_and_source_cells_match_comrak() {
    let fixtures = [
        "a | b\n--- | ---\n1 | 2\n",
        "| a | b |\n| :--- | ---: |\n| 1 | 2 |\n",
        "a\\|x | b\n:---: | ---\n1\\|2 | 3\n",
        "before\nheader | value\n--- | ---\nx | y\n",
    ];
    for source in fixtures {
        let machine = run(source, 11);
        assert_eq!(
            gate_tables(&machine),
            stock_tables(source),
            "source={source:?}\nfacts={:#?}",
            machine.facts()
        );
        for descriptor in machine.facts().iter().flat_map(|fact| match fact {
            CommitmentFact::TableStart { header, .. } => header.iter().collect::<Vec<_>>(),
            CommitmentFact::TableRow { cells, .. } => cells.iter().collect::<Vec<_>>(),
            _ => Vec::new(),
        }) {
            assert!(descriptor.source.start <= descriptor.source.end);
            assert!(descriptor.source.end <= source.len());
        }
    }
    let escaped = run("a\\|x | b\n--- | ---\n1\\|2 | 3\n", 5);
    assert!(escaped.facts().iter().any(|fact| match fact {
        CommitmentFact::TableStart { header, .. }
        | CommitmentFact::TableRow { cells: header, .. } => {
            header.iter().any(|cell| cell.escaped_pipe)
        }
        _ => false,
    }));
}

fn stock_html(source: &str) -> Vec<(u8, String)> {
    let arena = Arena::new();
    let root = parse_document(&arena, source, &options());
    root.descendants()
        .filter_map(|node| match &node.data().value {
            NodeValue::HtmlBlock(html) => Some((html.block_type, html.literal.clone())),
            _ => None,
        })
        .collect()
}

fn gate_html(machine: &CommitmentSpine) -> Vec<(u8, String)> {
    machine
        .facts()
        .iter()
        .filter_map(|fact| match fact {
            CommitmentFact::HtmlBlock {
                block_type, source, ..
            } => Some((*block_type, machine.source()[source.clone()].to_owned())),
            _ => None,
        })
        .collect()
}

#[test]
fn all_commonmark_html_types_and_same_line_terminators_match_comrak() {
    let fixtures = [
        "<script>x</script>after\nparagraph\n",
        "<!-- comment -->after\nparagraph\n",
        "<?pi?>after\nparagraph\n",
        "<!X>after\nparagraph\n",
        "<![CDATA[x]]>after\nparagraph\n",
        "<div>\nraw\n\nparagraph\n",
        "<x-tag a='b'>\nraw\n\nparagraph\n",
    ];
    for source in fixtures {
        let machine = run(source, 3);
        assert_eq!(
            gate_html(&machine),
            stock_html(source),
            "source={source:?}\nfacts={:#?}",
            machine.facts()
        );
    }
}

#[test]
fn reference_extraction_normalization_first_wins_and_generation_are_exact() {
    let source = concat!(
        "before [use][Straße]\n\n",
        "[STRASSE]: /first \"one\"\n",
        "[Straße]: /second \"two\"\n",
        "[multi]:\n  <https://example.com/a>\n  'title'\n\n",
        "after [use][strasse] and [m][multi]\n",
    );
    let machine = run(source, 13);
    assert_eq!(
        machine.materialize_reference_snapshot(),
        vec![
            (
                "multi".into(),
                "https://example.com/a".into(),
                "title".into()
            ),
            ("strasse".into(), "/first".into(), "one".into()),
        ]
    );
    assert_eq!(machine.state().symbol_generation(), 2);
    let definitions = machine
        .facts()
        .iter()
        .filter_map(|fact| match fact {
            CommitmentFact::ReferenceDefinition {
                normalized_label,
                first_definition,
                ..
            } => Some((normalized_label.to_string(), *first_definition)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        definitions,
        [
            ("strasse".into(), true),
            ("strasse".into(), false),
            ("multi".into(), true)
        ]
    );
    let dependencies = machine
        .facts()
        .iter()
        .filter_map(|fact| match fact {
            CommitmentFact::InlineDependency {
                symbol_generation, ..
            } => Some(*symbol_generation),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(dependencies, [0, 2]);
}

#[test]
fn checkpoints_resume_table_and_symbol_state_after_adversarial_suffix_edits() {
    let original = "[x]: /one\n\nhead | value\n--- | ---\na | b\ntail [x]\n";
    let edited = "[x]: /one\n\nhead | value\n:-- | --:\nc | d\ntail [x]\n";
    let boundary = original.find("---").unwrap();
    assert_eq!(boundary, edited.find(":--").unwrap());

    let mut prefix = CommitmentSpine::new(original);
    while prefix
        .checkpoint()
        .is_none_or(|checkpoint| checkpoint.offset < boundary)
    {
        prefix.advance(1);
    }
    let checkpoint = prefix.checkpoint().unwrap();
    assert_eq!(checkpoint.offset, boundary);
    let resumed = {
        let mut machine = CommitmentSpine::resume(edited, checkpoint);
        while !machine.is_complete() {
            machine.advance(7);
        }
        machine.advance(7);
        machine
    };
    let clean = run(edited, 7);
    assert!(resumed.state().semantic_eq(clean.state()));
    assert_eq!(gate_tables(&resumed), gate_tables(&clean));
    assert_eq!(
        resumed.materialize_reference_snapshot(),
        clean.materialize_reference_snapshot()
    );
}

#[test]
fn giant_lines_yield_then_degrade_explicitly_without_atomic_scanner_work() {
    for prefix in ["| ", "<script>", "[label]: "] {
        let source = format!("{prefix}{}\n", "x".repeat(10_000_000));
        let mut machine = CommitmentSpine::new(source);
        let mut polls = 0;
        let mut maximum_bytes = 0;
        while !machine.is_complete() {
            let report = machine.advance(4_096);
            assert!(report.work_units <= 4_096);
            assert!(report.source_bytes_inspected <= 4_096);
            maximum_bytes = maximum_bytes.max(report.source_bytes_inspected);
            polls += 1;
        }
        machine.advance(4_096);
        assert!(polls > 2_400);
        assert!(maximum_bytes <= 4_096);
        assert_eq!(machine.maximum_atomic_classification_bytes(), 0);
        assert!(matches!(
            machine.facts(),
            [CommitmentFact::Opaque {
                reason: OpaqueReason::PhysicalLineOverCap,
                ..
            }]
        ));
    }
}

#[test]
fn every_published_range_is_stable_and_in_bounds() {
    let source = "- one\n\n- two\n\na | b\n--- | ---\nx | y\n\n<!--x-->\n\n[k]: /u\n";
    let mut machine = run(source, 2);
    for fact in machine.facts() {
        let ranges = match fact {
            CommitmentFact::List { source, .. }
            | CommitmentFact::TableStart { source, .. }
            | CommitmentFact::TableRow { source, .. }
            | CommitmentFact::HtmlBlock { source, .. }
            | CommitmentFact::ReferenceDefinition { source, .. }
            | CommitmentFact::InlineDependency { source, .. }
            | CommitmentFact::Opaque { source, .. } => vec![source],
        };
        for range in ranges {
            assert!(range.start <= range.end, "{fact:?}");
            assert!(range.end <= source.len(), "{fact:?}");
            assert!(source.is_char_boundary(range.start), "{fact:?}");
            assert!(source.is_char_boundary(range.end), "{fact:?}");
        }
    }
    assert_eq!(machine.advance(1).status, CommitmentStatus::Complete);
}
