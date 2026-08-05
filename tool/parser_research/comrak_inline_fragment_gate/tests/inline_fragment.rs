use std::collections::HashMap;
use std::ops::Range;
use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use comrak::inline_fragment::{
    EMPTY_REFERENCE_SNAPSHOT, INLINE_FACT_FLAG_REFERENCE_SYMBOL, INLINE_FACT_FLAG_SOURCE_BACKED,
    InlineFact, InlineFactKind, InlineFragment, InlineFragmentError, InlineFragmentRequest,
    InlineInputKind, InlineProfile, InlineProjectionFactKind, InlineReferenceSnapshot,
    InlineReferenceTarget, MAX_INLINE_FRAGMENT_BYTES, parse_inline_fragment,
};
use comrak::nodes::{Node, NodeValue, Sourcepos};
use comrak::{Arena, Options, ResolvedReference, parse_document};

#[derive(Debug)]
struct TestSnapshot {
    identity: u64,
    generation: u64,
    entries: HashMap<String, ResolvedReference>,
    lookups: AtomicUsize,
}

impl TestSnapshot {
    fn new(entries: &[(&str, &str, &str)]) -> Self {
        let mut map = HashMap::new();
        for (normalized, url, title) in entries {
            map.entry((*normalized).to_owned())
                .or_insert_with(|| ResolvedReference {
                    url: (*url).to_owned(),
                    title: (*title).to_owned(),
                });
        }
        Self {
            identity: 71,
            generation: 9,
            entries: map,
            lookups: AtomicUsize::new(0),
        }
    }
}

impl InlineReferenceSnapshot for TestSnapshot {
    fn identity(&self) -> u64 {
        self.identity
    }

    fn generation(&self) -> u64 {
        self.generation
    }

    fn resolve(&self, normalized: &str, _original: &str) -> InlineReferenceTarget {
        self.lookups.fetch_add(1, Ordering::Relaxed);
        let defined = self.entries.contains_key(normalized);
        InlineReferenceTarget {
            symbol_id: test_symbol_id(normalized),
            presence_generation: usize::from(defined) as u64,
            defined,
        }
    }
}

#[derive(Debug, Default)]
struct RecordingSnapshot {
    queries: Mutex<Vec<String>>,
}

impl InlineReferenceSnapshot for RecordingSnapshot {
    fn identity(&self) -> u64 {
        5
    }

    fn generation(&self) -> u64 {
        1
    }

    fn resolve(&self, normalized: &str, _original: &str) -> InlineReferenceTarget {
        self.queries.lock().unwrap().push(normalized.to_owned());
        InlineReferenceTarget {
            symbol_id: test_symbol_id(normalized),
            presence_generation: 0,
            defined: false,
        }
    }
}

fn test_symbol_id(label: &str) -> u64 {
    label
        .as_bytes()
        .iter()
        .fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
        })
}

fn parse(source: &str, kind: InlineInputKind, profile: InlineProfile) -> InlineFragment {
    parse_inline_fragment(InlineFragmentRequest {
        logical: source,
        leaf_id: 11,
        kind,
        profile,
        reference_snapshot: &EMPTY_REFERENCE_SNAPSHOT,
        revision: 7,
        expected_revision: 7,
    })
    .unwrap()
}

fn options(profile: InlineProfile) -> Options<'static> {
    let mut options = Options::default();
    if profile == InlineProfile::Gfm {
        options.extension.strikethrough = true;
        options.extension.tagfilter = true;
        options.extension.table = true;
        options.extension.autolink = true;
        options.extension.tasklist = true;
    }
    options
}

#[derive(Debug, PartialEq, Eq)]
struct Signature {
    kind: u8,
    flags: u8,
    depth: u16,
    range: Range<usize>,
    payload: Vec<u8>,
}

fn fragment_signatures(fragment: &InlineFragment, source: &str) -> Vec<Signature> {
    fragment
        .facts
        .iter()
        .map(|fact| {
            let range =
                fact.logical_start as usize..(fact.logical_start + fact.logical_len) as usize;
            let mut flags = fact.flags;
            let payload = if flags & INLINE_FACT_FLAG_SOURCE_BACKED != 0 {
                flags &= !INLINE_FACT_FLAG_SOURCE_BACKED;
                materialize_source_range(fragment, source, range.clone())
            } else {
                fragment.payload
                    [fact.payload_start as usize..(fact.payload_start + fact.payload_len) as usize]
                    .to_vec()
            };
            Signature {
                kind: fact.kind,
                flags,
                depth: fact.depth,
                range,
                payload,
            }
        })
        .collect()
}

fn materialize_source_range(
    fragment: &InlineFragment,
    source: &str,
    range: Range<usize>,
) -> Vec<u8> {
    let mut projections = fragment
        .projection_facts
        .iter()
        .filter(|fact| {
            let start = fact.logical_start as usize;
            let end = (fact.logical_start + fact.logical_len) as usize;
            start >= range.start && end <= range.end
        })
        .collect::<Vec<_>>();
    projections.sort_by_key(|fact| fact.logical_start);
    let mut materialized = Vec::new();
    let mut cursor = range.start;
    for projection in projections {
        let start = projection.logical_start as usize;
        let end = (projection.logical_start + projection.logical_len) as usize;
        assert!(start >= cursor);
        materialized.extend_from_slice(&source.as_bytes()[cursor..start]);
        if projection.kind == InlineProjectionFactKind::Replacement as u8 {
            materialized.extend_from_slice(
                &fragment.payload[projection.payload_start as usize
                    ..(projection.payload_start + projection.payload_len) as usize],
            );
        }
        cursor = end;
    }
    materialized.extend_from_slice(&source.as_bytes()[cursor..range.end]);
    materialized
}

fn dereferenced_signatures(
    fragment: &InlineFragment,
    source: &str,
    snapshot: &TestSnapshot,
) -> Vec<Signature> {
    let mut signatures = fragment_signatures(fragment, source);
    for signature in &mut signatures {
        if signature.flags & INLINE_FACT_FLAG_REFERENCE_SYMBOL == 0 {
            continue;
        }
        let symbol_id = u64::from_le_bytes(signature.payload.clone().try_into().unwrap());
        let dependency = fragment
            .reference_dependencies
            .iter()
            .find(|dependency| dependency.symbol_id == symbol_id)
            .unwrap();
        let reference = snapshot.entries.get(&dependency.normalized_label).unwrap();
        signature.payload = encode_link(&reference.url, &reference.title);
        signature.flags &= !INLINE_FACT_FLAG_REFERENCE_SYMBOL;
    }
    signatures
}

fn first_inline_parent<'a>(root: Node<'a>) -> Option<Node<'a>> {
    root.descendants().find(|node| {
        matches!(
            node.data().value,
            NodeValue::Paragraph | NodeValue::Heading(_) | NodeValue::TableCell
        )
    })
}

fn stock_signatures(parent: Node<'_>, source: &str) -> Vec<Signature> {
    let starts = line_starts(source);
    parent
        .descendants()
        .skip(1)
        .map(|node| {
            let ast = node.data();
            let (kind, payload) = encode_stock(&ast.value);
            Signature {
                kind,
                flags: 0,
                depth: (node.ancestors().skip(1).count() - 1) as u16,
                range: sourcepos_range(ast.sourcepos, source, &starts),
                payload,
            }
        })
        .collect()
}

fn encode_stock(value: &NodeValue) -> (u8, Vec<u8>) {
    match value {
        NodeValue::Text(text) => (InlineFactKind::Text as u8, text.as_bytes().to_vec()),
        NodeValue::SoftBreak => (InlineFactKind::SoftBreak as u8, vec![]),
        NodeValue::LineBreak => (InlineFactKind::LineBreak as u8, vec![]),
        NodeValue::Code(code) => {
            let mut payload = (code.num_backticks as u32).to_le_bytes().to_vec();
            payload.extend_from_slice(code.literal.as_bytes());
            (InlineFactKind::Code as u8, payload)
        }
        NodeValue::HtmlInline(html) => (InlineFactKind::HtmlInline as u8, html.as_bytes().to_vec()),
        NodeValue::Emph => (InlineFactKind::Emphasis as u8, vec![]),
        NodeValue::Strong => (InlineFactKind::Strong as u8, vec![]),
        NodeValue::Strikethrough => (InlineFactKind::Strikethrough as u8, vec![]),
        NodeValue::Link(link) => (
            InlineFactKind::Link as u8,
            encode_link(&link.url, &link.title),
        ),
        NodeValue::Image(link) => (
            InlineFactKind::Image as u8,
            encode_link(&link.url, &link.title),
        ),
        NodeValue::Escaped => (InlineFactKind::Escaped as u8, vec![]),
        other => panic!("unsupported stock inline node {other:?}"),
    }
}

fn encode_link(url: &str, title: &str) -> Vec<u8> {
    let mut payload = (url.len() as u32).to_le_bytes().to_vec();
    payload.extend_from_slice(url.as_bytes());
    payload.extend_from_slice(title.as_bytes());
    payload
}

fn line_starts(source: &str) -> Vec<usize> {
    let bytes = source.as_bytes();
    let mut starts = vec![0];
    let mut offset = 0;
    while offset < bytes.len() {
        match bytes[offset] {
            b'\r' if bytes.get(offset + 1) == Some(&b'\n') => {
                offset += 2;
                starts.push(offset);
            }
            b'\r' | b'\n' => {
                offset += 1;
                starts.push(offset);
            }
            _ => offset += 1,
        }
    }
    starts
}

fn sourcepos_range(sourcepos: Sourcepos, source: &str, starts: &[usize]) -> Range<usize> {
    let start = starts[sourcepos.start.line - 1] + sourcepos.start.column - 1;
    let end = starts[sourcepos.end.line - 1] + sourcepos.end.column;
    assert!(end <= source.len(), "{sourcepos:?} > {}", source.len());
    start..end
}

fn assert_same_single_leaf(source: &str, profile: InlineProfile) {
    let options = options(profile);
    let arena = Arena::new();
    let root = parse_document(&arena, source, &options);
    let parent = first_inline_parent(root).expect("one inline parent");
    let stock = stock_signatures(parent, source);
    let fragment = parse(source, InlineInputKind::Paragraph, profile);
    assert_eq!(
        normalize_text_facts(fragment_signatures(&fragment, source)),
        normalize_text_facts(stock),
        "source={source:?}"
    );
}

fn normalize_text_facts(facts: Vec<Signature>) -> Vec<Signature> {
    let mut normalized: Vec<Signature> = Vec::with_capacity(facts.len());
    let mut escaped_depths = Vec::new();
    for mut fact in facts {
        while escaped_depths
            .last()
            .is_some_and(|escaped_depth| fact.depth <= *escaped_depth)
        {
            escaped_depths.pop();
        }
        if fact.kind == InlineFactKind::Escaped as u8 {
            escaped_depths.push(fact.depth);
            continue;
        }
        fact.depth -= escaped_depths.len() as u16;
        if fact.kind == InlineFactKind::Text as u8
            && let Some(previous) = normalized.last_mut()
            && previous.kind == fact.kind
            && previous.depth == fact.depth
            && previous.range.end <= fact.range.start
        {
            previous.range.end = fact.range.end;
            previous.payload.extend_from_slice(&fact.payload);
            continue;
        }
        normalized.push(fact);
    }
    normalized
}

#[test]
fn curated_commonmark_and_gfm_match_stock_nodes_ranges_and_payloads() {
    let commonmark = [
        "plain text",
        "*em* and **strong** and ***both***",
        "a_b_c and a***b***c",
        "`code` and `` code ` inside ``",
        "[link](https://example.com \"title\")",
        "![alt *em*](image.png 'title')",
        "<https://example.com> <me@example.com>",
        "<span title='x'>raw</span>",
        "\\*escaped\\* &copy; &#x1F600;",
        "first  \r\nsecond\r\nthird",
        "a\n",
        "a  \n",
        "a\\\n",
        "a\r\nb",
        "a  \r\nb",
        "a\r\nb  \r\n",
        "[outer *nested [inner](u)*](v)",
        "unmatched *** [bracket `tick",
        "Unicode émoji 😀 **世界**",
    ];
    for source in commonmark {
        assert_same_single_leaf(source, InlineProfile::CommonMark);
    }
    let gfm = [
        "~~strike *inside*~~",
        "www.example.com and https://example.com/path?q=1",
        "user@example.com and ~~[link](u)~~",
        // GFM example 631. Underscore delimiter processing leaves the address
        // in adjacent Text nodes; stock Comrak coalesces before autolinking.
        "a.b-c_d@a.b",
        "<xmp>raw</xmp> ~~done~~",
    ];
    for source in gfm {
        assert_same_single_leaf(source, InlineProfile::Gfm);
    }
}

#[test]
fn terminal_suffix_is_rtrimmed_but_interior_lf_and_crlf_remain_logical() {
    for source in ["a\n", "a  \n", "a\\\n", "a\r\n", "a  \r\n"] {
        let fragment = parse(
            source,
            InlineInputKind::Paragraph,
            InlineProfile::CommonMark,
        );
        assert!(fragment.facts.iter().all(|fact| {
            fact.kind != InlineFactKind::SoftBreak as u8
                && fact.kind != InlineFactKind::LineBreak as u8
        }));
        assert!(fragment.projection_facts.is_empty());
    }

    let soft = parse(
        "a\r\nb",
        InlineInputKind::Paragraph,
        InlineProfile::CommonMark,
    );
    let soft = soft
        .facts
        .iter()
        .find(|fact| fact.kind == InlineFactKind::SoftBreak as u8)
        .unwrap();
    assert_eq!(soft.logical_start, 1);
    assert_eq!(soft.logical_len, 2);

    let hard = parse(
        "a  \r\nb",
        InlineInputKind::Paragraph,
        InlineProfile::CommonMark,
    );
    let hard_break = hard
        .facts
        .iter()
        .find(|fact| fact.kind == InlineFactKind::LineBreak as u8)
        .unwrap();
    assert_eq!(hard_break.logical_start, 1);
    assert_eq!(hard_break.logical_len, 4);
    assert!(hard.projection_facts.iter().any(|fact| {
        fact.kind == InlineProjectionFactKind::HiddenMarker as u8
            && fact.logical_start == 1
            && fact.logical_len == 2
    }));
}

#[test]
fn exhaustive_atom_pairs_match_stock() {
    let atoms = [
        "a",
        "*b*",
        "**c**",
        "`d`",
        "[e](u)",
        "![f](i)",
        "&copy;",
        "\\*",
        "<b>",
        "~~g~~",
        "www.example.com",
    ];
    for left in atoms {
        for middle in ["", " ", "_", "*"] {
            for right in atoms {
                let source = format!("{left}{middle}{right}");
                assert_same_single_leaf(&source, InlineProfile::Gfm);
            }
        }
    }
}

#[test]
fn first_definition_wins_reference_snapshot_matches_full_document() {
    let leaf = "[use][Straße] and ![img][pic]";
    let definitions = TestSnapshot::new(&[
        ("strasse", "/first", "one"),
        ("strasse", "/second", "two"),
        ("pic", "/p.png", ""),
    ]);
    let fragment = parse_inline_fragment(InlineFragmentRequest {
        logical: leaf,
        leaf_id: 12,
        kind: InlineInputKind::Paragraph,
        profile: InlineProfile::Gfm,
        reference_snapshot: &definitions,
        revision: 3,
        expected_revision: 3,
    })
    .unwrap();

    let document = "[STRASSE]: /first \"one\"\n[Straße]: /second \"two\"\n[pic]: /p.png\n\n[use][Straße] and ![img][pic]";
    let arena = Arena::new();
    let root = parse_document(&arena, document, &options(InlineProfile::Gfm));
    let parent = root
        .descendants()
        .find(|node| matches!(node.data().value, NodeValue::Paragraph))
        .unwrap();
    let mut stock = stock_signatures(parent, document);
    let physical_start = document.find(leaf).unwrap();
    for signature in &mut stock {
        signature.range =
            signature.range.start - physical_start..signature.range.end - physical_start;
    }
    assert_eq!(
        dereferenced_signatures(&fragment, leaf, &definitions),
        stock
    );
    assert_eq!(fragment.reference_snapshot_identity, 71);
    assert_eq!(fragment.reference_snapshot_generation, 9);
    assert_eq!(definitions.lookups.load(Ordering::Relaxed), 2);
    assert_eq!(
        fragment.reference_dependencies,
        vec![
            comrak::inline_fragment::ReferenceDependency {
                symbol_id: test_symbol_id("pic"),
                presence_generation: 1,
                normalized_label: "pic".into(),
                resolved: true,
            },
            comrak::inline_fragment::ReferenceDependency {
                symbol_id: test_symbol_id("strasse"),
                presence_generation: 1,
                normalized_label: "strasse".into(),
                resolved: true,
            },
        ]
    );
}

#[test]
fn reference_dependencies_report_deduplicated_hits_and_misses() {
    let snapshot = TestSnapshot::new(&[("hit", "/ok", "")]);
    let source = "[a][hit] [b][miss] [again][hit]";
    let fragment = parse_inline_fragment(InlineFragmentRequest {
        logical: source,
        leaf_id: 7,
        kind: InlineInputKind::Paragraph,
        profile: InlineProfile::Gfm,
        reference_snapshot: &snapshot,
        revision: 8,
        expected_revision: 8,
    })
    .unwrap();
    assert_eq!(
        fragment.reference_dependencies,
        vec![
            comrak::inline_fragment::ReferenceDependency {
                symbol_id: test_symbol_id("hit"),
                presence_generation: 1,
                normalized_label: "hit".into(),
                resolved: true,
            },
            comrak::inline_fragment::ReferenceDependency {
                symbol_id: test_symbol_id("miss"),
                presence_generation: 0,
                normalized_label: "miss".into(),
                resolved: false,
            },
        ]
    );
    assert_eq!(snapshot.lookups.load(Ordering::Relaxed), 4);
}

#[test]
fn bracket_after_paths_never_query_an_empty_noncandidate_label() {
    for (source, expected) in [
        ("]", Vec::<&str>::new()),
        ("[a][b][c]", vec!["b", "c", "c"]),
        ("[outer [inner]]", vec!["inner"]),
        ("![][x][y]", vec!["x", "y", "y"]),
    ] {
        let snapshot = RecordingSnapshot::default();
        parse_inline_fragment(InlineFragmentRequest {
            logical: source,
            leaf_id: 6,
            kind: InlineInputKind::Paragraph,
            profile: InlineProfile::Gfm,
            reference_snapshot: &snapshot,
            revision: 1,
            expected_revision: 1,
        })
        .unwrap();
        let queries = snapshot.queries.lock().unwrap();
        assert_eq!(
            queries.as_slice(),
            expected,
            "unexpected resolver candidates for {source:?}"
        );
    }
}

#[test]
fn reference_value_changes_are_symbol_indirected_but_presence_changes_reparse() {
    let source = "before [label][target] after";
    let old = TestSnapshot {
        identity: 90,
        generation: 10,
        entries: HashMap::from([(
            "target".into(),
            ResolvedReference {
                url: "/old".into(),
                title: "old title".into(),
            },
        )]),
        lookups: AtomicUsize::new(0),
    };
    let new = TestSnapshot {
        identity: 90,
        generation: 11,
        entries: HashMap::from([(
            "target".into(),
            ResolvedReference {
                url: "/new".into(),
                title: "new title".into(),
            },
        )]),
        lookups: AtomicUsize::new(0),
    };
    let undefined = TestSnapshot {
        identity: 90,
        generation: 12,
        entries: HashMap::new(),
        lookups: AtomicUsize::new(0),
    };

    let parse_with = |snapshot: &TestSnapshot| {
        parse_inline_fragment(InlineFragmentRequest {
            logical: source,
            leaf_id: 19,
            kind: InlineInputKind::Paragraph,
            profile: InlineProfile::Gfm,
            reference_snapshot: snapshot,
            revision: 4,
            expected_revision: 4,
        })
        .unwrap()
    };
    let old_leaf = parse_with(&old);
    let new_leaf = parse_with(&new);
    assert_eq!(old_leaf.facts, new_leaf.facts);
    assert_eq!(old_leaf.projection_facts, new_leaf.projection_facts);
    assert_eq!(old_leaf.payload, new_leaf.payload);
    assert_eq!(
        old_leaf.reference_dependencies,
        new_leaf.reference_dependencies
    );
    assert_ne!(
        old_leaf.reference_snapshot_generation,
        new_leaf.reference_snapshot_generation
    );
    let reference_fact = old_leaf
        .facts
        .iter()
        .find(|fact| fact.kind == InlineFactKind::Link as u8)
        .unwrap();
    assert_eq!(reference_fact.flags, INLINE_FACT_FLAG_REFERENCE_SYMBOL);
    assert_eq!(reference_fact.payload_len, 8);

    let undefined_leaf = parse_with(&undefined);
    assert_ne!(undefined_leaf.facts, new_leaf.facts);
    assert_eq!(undefined_leaf.reference_dependencies.len(), 1);
    assert_eq!(
        undefined_leaf.reference_dependencies[0].symbol_id,
        test_symbol_id("target")
    );
    assert_eq!(
        undefined_leaf.reference_dependencies[0].presence_generation,
        0
    );
    assert!(!undefined_leaf.reference_dependencies[0].resolved);
    assert_eq!(new_leaf.reference_dependencies[0].presence_generation, 1);
    assert!(new_leaf.reference_dependencies[0].resolved);
}

#[test]
fn tiny_reference_leaf_never_clones_a_huge_definition_value() {
    let snapshot = TestSnapshot {
        identity: 91,
        generation: 2,
        entries: HashMap::from([(
            "huge".into(),
            ResolvedReference {
                url: "u".repeat(4 * 1024 * 1024),
                title: "t".repeat(4 * 1024 * 1024),
            },
        )]),
        lookups: AtomicUsize::new(0),
    };
    let fragment = parse_inline_fragment(InlineFragmentRequest {
        logical: "[x][huge]",
        leaf_id: 22,
        kind: InlineInputKind::Paragraph,
        profile: InlineProfile::Gfm,
        reference_snapshot: &snapshot,
        revision: 1,
        expected_revision: 1,
    })
    .unwrap();
    assert_eq!(snapshot.lookups.load(Ordering::Relaxed), 1);
    let link = fragment
        .facts
        .iter()
        .find(|fact| fact.kind == InlineFactKind::Link as u8)
        .unwrap();
    assert_eq!(link.flags, INLINE_FACT_FLAG_REFERENCE_SYMBOL);
    assert_eq!(link.payload_len, 8);
    assert!(fragment.payload.len() < 64);
    assert!(fragment.output_bytes() < 256);
}

#[test]
fn local_leaf_with_one_hundred_thousand_references_performs_one_lookup() {
    let mut entries = HashMap::with_capacity(100_000);
    for index in 0..100_000 {
        entries.insert(
            format!("label-{index}"),
            ResolvedReference {
                url: format!("/url/{index}"),
                title: String::new(),
            },
        );
    }
    let snapshot = TestSnapshot {
        identity: 800,
        generation: 33,
        entries,
        lookups: AtomicUsize::new(0),
    };
    let source = "[use][label-99999]";
    let started = Instant::now();
    let fragment = parse_inline_fragment(InlineFragmentRequest {
        logical: source,
        leaf_id: 3,
        kind: InlineInputKind::Paragraph,
        profile: InlineProfile::Gfm,
        reference_snapshot: &snapshot,
        revision: 4,
        expected_revision: 4,
    })
    .unwrap();
    let elapsed = started.elapsed();
    assert_eq!(snapshot.lookups.load(Ordering::Relaxed), 1);
    assert_eq!(fragment.reference_snapshot_identity, 800);
    assert_eq!(fragment.reference_snapshot_generation, 33);
    assert!(
        fragment
            .facts
            .iter()
            .any(|fact| fact.kind == InlineFactKind::Link as u8)
    );
    eprintln!("100k-reference local leaf: {elapsed:?}, one resolver lookup");
}

#[test]
fn projection_annotations_are_leaf_logical_and_do_not_claim_physical_prefix_gaps() {
    use flark_comrak_inline_fragment_gate::{LogicalOriginMap, LogicalOriginRun, OriginRunKind};

    // Physical source: `> *bold\r\n> rest* and \\* &copy;`. The authoritative block
    // leaf removes both quote prefixes and normalizes CRLF to one logical LF.
    let logical = "*bold\nrest* and \\* &copy;";
    let fragment = parse_inline_fragment(InlineFragmentRequest {
        logical,
        leaf_id: 99,
        kind: InlineInputKind::ListItemParagraph,
        profile: InlineProfile::Gfm,
        reference_snapshot: &EMPTY_REFERENCE_SNAPSHOT,
        revision: 9,
        expected_revision: 9,
    })
    .unwrap();
    let map = LogicalOriginMap {
        leaf_id: 99,
        revision: 9,
        logical_len: logical.len() as u32,
        runs: vec![
            LogicalOriginRun {
                logical: 0..5,
                physical: 2..7,
                kind: OriginRunKind::Identity,
            },
            LogicalOriginRun {
                logical: 5..6,
                physical: 7..9,
                kind: OriginRunKind::Atomic,
            },
            LogicalOriginRun {
                logical: 6..logical.len() as u32,
                physical: 11..11 + (logical.len() - 6) as u64,
                kind: OriginRunKind::Identity,
            },
        ],
    };
    let composed = map.compose(&fragment).unwrap();
    let claimed: Vec<_> = composed
        .semantic_facts
        .iter()
        .chain(&composed.projection_facts)
        .flat_map(|fact| fact.physical_parts.iter())
        .cloned()
        .collect();
    assert!(
        claimed
            .iter()
            .all(|part| !(part.start < 11 && part.end > 9))
    );
    assert!(claimed.iter().all(|part| !(part.start < 2 && part.end > 0)));
    assert!(
        composed
            .semantic_facts
            .iter()
            .any(|fact| fact.physical_parts.len() == 3)
    );
}

#[test]
fn origin_map_rejects_partial_splits_of_atomic_replacements() {
    use flark_comrak_inline_fragment_gate::{
        LogicalOriginMap, LogicalOriginRun, OriginMapError, OriginRunKind,
    };
    let map = LogicalOriginMap {
        leaf_id: 1,
        revision: 1,
        logical_len: 3,
        runs: vec![LogicalOriginRun {
            logical: 0..3,
            physical: 40..41,
            kind: OriginRunKind::Atomic,
        }],
    };
    let fact = InlineFact {
        kind: InlineFactKind::Text as u8,
        flags: 0,
        depth: 1,
        logical_start: 1,
        logical_len: 1,
        payload_start: 0,
        payload_len: 0,
    };
    assert!(matches!(
        map.map_fact(fact),
        Err(OriginMapError::PartialAtomicMapping { .. })
    ));
}

#[test]
fn headings_list_items_and_table_cells_use_the_same_inline_grammar() {
    let logical = "**bold** and ~~strike~~";
    let expected = parse(logical, InlineInputKind::Paragraph, InlineProfile::Gfm);
    for kind in [
        InlineInputKind::Heading {
            level: 2,
            setext: false,
        },
        InlineInputKind::ListItemParagraph,
        InlineInputKind::TableCell,
    ] {
        let actual = parse(logical, kind, InlineProfile::Gfm);
        assert_eq!(
            fragment_signatures(&actual, logical),
            fragment_signatures(&expected, logical)
        );
    }
}

#[test]
fn cap_kind_revision_and_map_fail_closed() {
    let over = "x".repeat(MAX_INLINE_FRAGMENT_BYTES + 1);
    let error = parse_inline_fragment(InlineFragmentRequest {
        logical: &over,
        leaf_id: 1,
        kind: InlineInputKind::Paragraph,
        profile: InlineProfile::CommonMark,
        reference_snapshot: &EMPTY_REFERENCE_SNAPSHOT,
        revision: 1,
        expected_revision: 1,
    })
    .unwrap_err();
    assert!(matches!(error, InlineFragmentError::OverCap { .. }));

    let source = "text";
    for kind in [
        InlineInputKind::RawBlock,
        InlineInputKind::ReferenceDefinition,
        InlineInputKind::Document,
        InlineInputKind::Heading {
            level: 9,
            setext: false,
        },
    ] {
        let error = parse_inline_fragment(InlineFragmentRequest {
            logical: source,
            leaf_id: 1,
            kind,
            profile: InlineProfile::CommonMark,
            reference_snapshot: &EMPTY_REFERENCE_SNAPSHOT,
            revision: 1,
            expected_revision: 1,
        })
        .unwrap_err();
        assert_eq!(error, InlineFragmentError::UnsupportedInputKind(kind));
    }

    let error = parse_inline_fragment(InlineFragmentRequest {
        logical: source,
        leaf_id: 1,
        kind: InlineInputKind::Paragraph,
        profile: InlineProfile::CommonMark,
        reference_snapshot: &EMPTY_REFERENCE_SNAPSHOT,
        revision: 1,
        expected_revision: 2,
    })
    .unwrap_err();
    assert_eq!(
        error,
        InlineFragmentError::StaleRevision {
            leaf_id: 1,
            actual: 1,
            expected: 2,
        }
    );
}

#[test]
fn packed_fact_layout_is_fixed_and_output_is_bounded_for_dense_input() {
    assert_eq!(std::mem::size_of::<InlineFact>(), 20);
    let atom = "**bold** *em* `code` [link](u) ~~strike~~ ";
    let mut source = String::new();
    while source.len() + atom.len() <= MAX_INLINE_FRAGMENT_BYTES {
        source.push_str(atom);
    }
    source.push_str(&"x".repeat(MAX_INLINE_FRAGMENT_BYTES - source.len()));
    let fragment = parse(&source, InlineInputKind::Paragraph, InlineProfile::Gfm);
    assert!(!fragment.facts.is_empty());
    // Measured protocol output is explicit; this is a regression bound rather
    // than a claim that all syntax produces fewer output bytes than source.
    assert!(fragment.output_bytes() < 256 * 1024);
}

#[test]
fn text_facts_are_source_backed_and_reconstruct_through_projection() {
    let source = "plain &copy; and \\*escaped\\*";
    let fragment = parse(
        source,
        InlineInputKind::Paragraph,
        InlineProfile::CommonMark,
    );
    let text_facts = fragment
        .facts
        .iter()
        .filter(|fact| fact.kind == InlineFactKind::Text as u8)
        .collect::<Vec<_>>();
    assert!(!text_facts.is_empty());
    assert!(
        text_facts
            .iter()
            .all(|fact| { fact.flags == INLINE_FACT_FLAG_SOURCE_BACKED && fact.payload_len == 0 })
    );
    let materialized = fragment_signatures(&fragment, source)
        .into_iter()
        .filter(|fact| fact.kind == InlineFactKind::Text as u8)
        .flat_map(|fact| fact.payload)
        .collect::<Vec<_>>();
    assert_eq!(
        String::from_utf8(materialized).unwrap(),
        "plain © and *escaped*"
    );
}

type MarkerRanges = Vec<Range<usize>>;
type Replacements = Vec<(Range<usize>, String)>;

fn projection(
    source: &str,
    profile: InlineProfile,
    reference_snapshot: Option<&dyn InlineReferenceSnapshot>,
) -> (MarkerRanges, Replacements) {
    let fragment = parse_inline_fragment(InlineFragmentRequest {
        logical: source,
        leaf_id: 44,
        kind: InlineInputKind::Paragraph,
        profile,
        reference_snapshot: reference_snapshot.unwrap_or(&EMPTY_REFERENCE_SNAPSHOT),
        revision: 5,
        expected_revision: 5,
    })
    .unwrap();
    let mut markers = Vec::new();
    let mut replacements = Vec::new();
    for fact in &fragment.projection_facts {
        let range = fact.logical_start as usize..(fact.logical_start + fact.logical_len) as usize;
        match fact.kind {
            kind if kind == InlineProjectionFactKind::HiddenMarker as u8 => markers.push(range),
            kind if kind == InlineProjectionFactKind::Replacement as u8 => {
                let bytes = &fragment.payload
                    [fact.payload_start as usize..(fact.payload_start + fact.payload_len) as usize];
                replacements.push((range, String::from_utf8(bytes.to_vec()).unwrap()));
            }
            kind => panic!("unknown projection fact {kind}"),
        }
    }
    (markers, replacements)
}

#[test]
fn parser_owned_delimiter_annotations_cover_residual_nested_runs() {
    assert_eq!(
        projection("**foo *bar***", InlineProfile::Gfm, None).0,
        vec![0..2, 6..7, 10..11, 11..13]
    );
    assert_eq!(
        projection("***x***", InlineProfile::Gfm, None).0,
        vec![0..1, 1..3, 4..6, 6..7]
    );
    assert_eq!(
        projection("~~*x*~~", InlineProfile::Gfm, None).0,
        vec![0..2, 2..3, 4..5, 5..7]
    );
}

#[test]
fn parser_owned_escape_hardbreak_code_and_entity_projection_is_exact() {
    assert_eq!(
        projection("a\\\nb", InlineProfile::CommonMark, None),
        (std::iter::once(1..2).collect(), vec![])
    );
    assert_eq!(
        projection("a  \nb", InlineProfile::CommonMark, None),
        (std::iter::once(1..3).collect(), vec![])
    );
    assert_eq!(
        projection("` foo `", InlineProfile::CommonMark, None),
        (vec![0..1, 6..7], vec![(1..6, "foo".into())])
    );
    assert_eq!(
        projection("`foo\nbar`", InlineProfile::CommonMark, None),
        (vec![0..1, 8..9], vec![(1..8, "foo bar".into())])
    );
    assert_eq!(
        projection("&copy; &amp; &#x1F600;", InlineProfile::CommonMark, None),
        (
            vec![],
            vec![
                (0..6, "©".into()),
                (7..12, "&".into()),
                (13..22, "😀".into()),
            ],
        )
    );
    assert_eq!(
        projection("\\&copy;", InlineProfile::CommonMark, None),
        (std::iter::once(0..1).collect(), vec![])
    );
    assert_eq!(
        projection("[&copy;](u)", InlineProfile::CommonMark, None),
        (vec![0..1, 7..11], vec![(1..7, "©".into())])
    );
}

#[test]
fn parser_owned_link_annotations_cover_inline_reference_autolink_and_multiline_forms() {
    assert_eq!(
        projection("[Foo](https://en.org/Foo_(d))", InlineProfile::Gfm, None,).0,
        vec![0..1, 4..29]
    );
    assert_eq!(
        projection("![](u.png)", InlineProfile::Gfm, None).0,
        vec![0..2, 2..10]
    );
    assert_eq!(
        projection("<https://example.com>", InlineProfile::Gfm, None).0,
        vec![0..1, 20..21]
    );
    assert!(
        projection("www.example.com", InlineProfile::Gfm, None)
            .0
            .is_empty()
    );

    let references = TestSnapshot::new(&[("bar", "/url", "")]);
    assert_eq!(
        projection("[foo][bar]", InlineProfile::Gfm, Some(&references)).0,
        vec![0..1, 4..10]
    );
    let shortcut = TestSnapshot::new(&[("foo", "/url", "")]);
    assert_eq!(
        projection("[foo]", InlineProfile::Gfm, Some(&shortcut)).0,
        vec![0..1, 4..5]
    );
    assert_eq!(
        projection("[foo][]", InlineProfile::Gfm, Some(&shortcut)).0,
        vec![0..1, 4..7]
    );
    assert_eq!(
        projection("[*foo\nbar*](u)", InlineProfile::Gfm, None).0,
        vec![0..1, 1..2, 9..10, 10..14]
    );
    assert_eq!(
        projection("[foo\n](u)", InlineProfile::Gfm, None).0,
        vec![0..1, 5..9]
    );
    assert_eq!(
        projection("[a](u\n\"t\")", InlineProfile::Gfm, None).0,
        vec![0..1, 2..10]
    );
    let multiline = parse(
        "[a](u\n\"t\")",
        InlineInputKind::Paragraph,
        InlineProfile::Gfm,
    );
    let link = multiline
        .facts
        .iter()
        .find(|fact| fact.kind == InlineFactKind::Link as u8)
        .unwrap();
    assert_eq!(link.logical_start, 0);
    assert_eq!(link.logical_len, 10);
}

#[test]
fn task_context_is_block_owned_and_footnote_shaped_shortcut_stays_visible() {
    // The block spine certifies first-list-item-paragraph context. The inline
    // service owns strict task recognition and then exposes the remaining
    // paragraph through the ordinary inline grammar.
    assert_eq!(
        projection("**done**", InlineProfile::Gfm, None).0,
        vec![0..2, 6..8]
    );
    let footnote = TestSnapshot::new(&[("^1", "/not-a-footnote", "")]);
    assert!(
        projection("[^1]", InlineProfile::Gfm, Some(&footnote))
            .0
            .is_empty()
    );
    assert_eq!(
        projection("[^x](u)", InlineProfile::Gfm, None).0,
        vec![0..1, 3..7]
    );
}

#[test]
fn upstream_spec_single_paragraph_fixtures_match_stock_comrak() {
    let root =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../test/fixtures/commonmark/upstream");
    let commonmark = root.join("common_mark_tests.json");
    let gfm = root.join("gfm_tests.json");
    let mut compared = 0;
    for (path, profile) in [
        (commonmark, InlineProfile::CommonMark),
        (gfm, InlineProfile::Gfm),
    ] {
        let fixtures: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        for fixture in fixtures.as_array().unwrap() {
            let source = fixture["markdown"].as_str().unwrap();
            if source.len() > MAX_INLINE_FRAGMENT_BYTES
                || source.starts_with(' ')
                || source.starts_with('\t')
                || source.starts_with('\n')
                || source.starts_with('\r')
                || source.contains("\n\n")
                || source.contains("\r\r")
                || source.lines().any(|line| line.contains("]:"))
            {
                continue;
            }
            let arena = Arena::new();
            let root = parse_document(&arena, source, &options(profile));
            let children: Vec<_> = root.children().collect();
            if children.len() != 1 || !matches!(children[0].data().value, NodeValue::Paragraph) {
                continue;
            }
            let start = children[0].data().sourcepos.start;
            if start.line != 1 || start.column != 1 {
                continue;
            }
            let parent_range =
                sourcepos_range(children[0].data().sourcepos, source, &line_starts(source));
            if parent_range.start != 0 || parent_range.end < source.trim_end().len() {
                continue;
            }
            let stock = normalize_text_facts(stock_signatures(children[0], source));
            let fragment = parse(source, InlineInputKind::Paragraph, profile);
            let actual = normalize_text_facts(fragment_signatures(&fragment, source));
            assert_eq!(
                actual.len(),
                stock.len(),
                "example={} source={source:?}",
                fixture["example"]
            );
            for (actual, stock) in actual.iter().zip(&stock) {
                assert_eq!(
                    (actual.kind, actual.depth, &actual.payload),
                    (stock.kind, stock.depth, &stock.payload),
                    "profile={profile:?} example={} source={source:?}",
                    fixture["example"]
                );
                if matches!(actual.kind, kind if kind == InlineFactKind::Link as u8 || kind == InlineFactKind::Image as u8)
                {
                    assert_eq!(actual.range.start, stock.range.start);
                    assert!(actual.range.end >= stock.range.end);
                } else {
                    assert_eq!(actual.range, stock.range);
                }
            }
            compared += 1;
        }
    }
    eprintln!("upstream single-paragraph differentials: {compared}");
    assert!(compared >= 250, "only compared {compared} inline fixtures");
}
