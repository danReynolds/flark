use std::fs;
use std::path::PathBuf;

use comrak::nodes::NodeValue;
use comrak::{block_spine_facade, parse_document, Arena, Options};
use flark_comrak_derived_core_probe::commitment_spine::{
    Alignment, CommitmentFact, CommitmentSpine,
};
use serde_json::Value;

fn corpus() -> Vec<Value> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../test/fixtures/commonmark/upstream/gfm_tests.json");
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

fn options() -> Options<'static> {
    let mut options = Options::default();
    options.extension.table = true;
    options.render.r#unsafe = true;
    options
}

fn run(source: &str) -> CommitmentSpine {
    let mut machine = CommitmentSpine::new(source);
    while !machine.is_complete() {
        machine.advance(97);
    }
    machine.advance(97);
    machine
}

fn stock_html_blocks(source: &str) -> Vec<(u8, String)> {
    let arena = Arena::new();
    let root = parse_document(&arena, source, &options());
    root.descendants()
        .filter_map(|node| match &node.data().value {
            NodeValue::HtmlBlock(block) => Some((block.block_type, block.literal.clone())),
            _ => None,
        })
        .collect()
}

fn gate_html_blocks(source: &str) -> Vec<(u8, String)> {
    let machine = run(source);
    machine
        .facts()
        .iter()
        .filter_map(|fact| match fact {
            CommitmentFact::HtmlBlock {
                block_type,
                source: range,
                ..
            } => Some((*block_type, source[range.clone()].to_owned())),
            _ => None,
        })
        .collect()
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

fn gate_list_tightness(source: &str) -> Vec<bool> {
    let machine = run(source);
    let mut facts = machine
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
    facts.sort_by_key(|(start, depth, _)| (*start, *depth));
    facts.into_iter().map(|(_, _, tight)| tight).collect()
}

#[test]
fn pinned_gfm_html_block_section_records_known_pristine_comrak_divergences() {
    let mut failures = Vec::new();
    let mut checked = 0;
    for example in corpus() {
        if example["section"] != "HTML blocks" {
            continue;
        }
        checked += 1;
        let source = example["markdown"].as_str().unwrap();
        let stock = stock_html_blocks(source);
        let gate = gate_html_blocks(source);
        if gate != stock {
            failures.push((example["example"].as_u64().unwrap(), gate, stock));
        }
    }
    assert_eq!(checked, 43);
    let ids = failures.iter().map(|failure| failure.0).collect::<Vec<_>>();
    assert_eq!(ids, [120, 141, 142, 143, 144, 152, 153, 160]);
}

#[test]
fn pinned_gfm_list_section_records_known_pristine_comrak_divergences() {
    let mut failures = Vec::new();
    let mut checked = 0;
    for example in corpus() {
        if example["section"] != "Lists" {
            continue;
        }
        checked += 1;
        let source = example["markdown"].as_str().unwrap();
        let stock = stock_list_tightness(source);
        let gate = gate_list_tightness(source);
        if gate != stock {
            failures.push((
                example["example"].as_u64().unwrap(),
                source.to_owned(),
                gate,
                stock,
            ));
        }
    }
    assert_eq!(checked, 26);
    let ids = failures.iter().map(|failure| failure.0).collect::<Vec<_>>();
    assert_eq!(ids, [290, 291, 292, 293, 298, 305]);
}

#[derive(Debug, PartialEq, Eq)]
struct TableSignature {
    alignments: Vec<Alignment>,
    rows: usize,
    columns: usize,
}

fn stock_tables(source: &str) -> Vec<TableSignature> {
    let arena = Arena::new();
    let root = parse_document(&arena, source, &options());
    root.descendants()
        .filter_map(|node| match &node.data().value {
            NodeValue::Table(table) => Some(TableSignature {
                alignments: table
                    .alignments
                    .iter()
                    .map(|alignment| match alignment {
                        comrak::nodes::TableAlignment::None => Alignment::None,
                        comrak::nodes::TableAlignment::Left => Alignment::Left,
                        comrak::nodes::TableAlignment::Center => Alignment::Center,
                        comrak::nodes::TableAlignment::Right => Alignment::Right,
                    })
                    .collect(),
                rows: table.num_rows,
                columns: table.num_columns,
            }),
            _ => None,
        })
        .collect()
}

fn gate_tables(source: &str) -> Vec<TableSignature> {
    let machine = run(source);
    let mut result = Vec::<(u64, TableSignature)>::new();
    for fact in machine.facts() {
        match fact {
            CommitmentFact::TableStart {
                id,
                alignments,
                header,
                ..
            } => result.push((
                *id,
                TableSignature {
                    alignments: alignments.to_vec(),
                    rows: 1,
                    columns: header.len(),
                },
            )),
            CommitmentFact::TableRow { table_id, .. } => {
                result
                    .iter_mut()
                    .find(|(id, _)| id == table_id)
                    .unwrap()
                    .1
                    .rows += 1;
            }
            _ => {}
        }
    }
    result.into_iter().map(|(_, table)| table).collect()
}

#[test]
fn pinned_gfm_table_section_records_exact_under_cap_differentials() {
    let mut failures = Vec::new();
    let mut checked = 0;
    for example in corpus() {
        if example["section"] != "Tables (extension)" {
            continue;
        }
        checked += 1;
        let source = example["markdown"].as_str().unwrap();
        let stock = stock_tables(source);
        let gate = gate_tables(source);
        if gate != stock {
            failures.push((example["example"].as_u64().unwrap(), gate, stock));
        }
    }
    assert_eq!(checked, 8);
    let ids = failures.iter().map(|failure| failure.0).collect::<Vec<_>>();
    assert_eq!(ids, [201]);
}

fn candidate_reference_labels(source: &str) -> Vec<(String, String)> {
    let mut labels = Vec::new();
    for (offset, byte) in source.bytes().enumerate() {
        if byte != b'[' || !source.is_char_boundary(offset) {
            continue;
        }
        let tail = &source[offset..];
        if tail.len() > block_spine_facade::MAX_CLASSIFICATION_BYTES {
            continue;
        }
        let Ok(definitions) = block_spine_facade::reference_definitions(tail) else {
            continue;
        };
        if let Some(definition) = definitions.first() {
            let raw = tail[definition.label_source.clone()].to_owned();
            labels.push((definition.normalized_label.clone(), raw));
        }
    }
    labels.sort();
    labels.dedup_by(|left, right| left.0 == right.0);
    labels
}

fn stock_reference(source: &str, raw_label: &str) -> Option<(String, String)> {
    let probe = format!("{source}\n\n[flark-probe][{raw_label}]\n");
    let arena = Arena::new();
    let root = parse_document(&arena, &probe, &options());
    root.descendants()
        .filter_map(|node| match &node.data().value {
            NodeValue::Link(link) => Some((link.url.clone(), link.title.clone())),
            _ => None,
        })
        .last()
}

#[test]
fn pinned_reference_definition_section_records_symbol_snapshot_differentials() {
    let mut failures = Vec::new();
    let mut checked = 0;
    for example in corpus() {
        if example["section"] != "Link reference definitions" {
            continue;
        }
        checked += 1;
        let source = example["markdown"].as_str().unwrap();
        let machine = run(source);
        let actual = machine
            .materialize_reference_snapshot()
            .into_iter()
            .map(|(label, url, title)| (label, Some((url, title))))
            .collect::<std::collections::BTreeMap<_, _>>();
        let expected = candidate_reference_labels(source)
            .into_iter()
            .map(|(normalized, raw)| (normalized, stock_reference(source, &raw)))
            .filter(|(_, resolved)| resolved.is_some())
            .collect::<std::collections::BTreeMap<_, _>>();
        if actual != expected {
            failures.push((
                example["example"].as_u64().unwrap(),
                source.to_owned(),
                actual,
                expected,
            ));
        }
    }
    assert_eq!(checked, 28);
    let ids = failures.iter().map(|failure| failure.0).collect::<Vec<_>>();
    assert_eq!(ids, [183, 187]);
}
