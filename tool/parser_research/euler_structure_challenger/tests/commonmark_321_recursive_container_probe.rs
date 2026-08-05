use comrak::{Options as ComrakOptions, markdown_to_html};
use euler_structure_challenger::serialized_green::{
    Affinity, CoverageAtom, CoveragePart, GreenKind, GreenMutationReceipt, GreenProperty,
    GreenToken, PropertyTag, SerializedGreenSequence, SourceCoordinate,
};
use euler_structure_challenger::{BlockId, ClosedChildSummary};
use flark_engine::SourceStore;
use flark_parser::{
    M11CleanDocumentKind, M11CleanParseJob, M11CleanParsePoll, M11ListUnsupportedReason,
    M11UnknownReason,
};

const COMMONMARK_321: &str = "- a\n  > b\n  ```\n  c\n  ```\n- d\n";
const COMMONMARK_325: &str = "* foo\n  * bar\n\n  baz\n";

const DOCUMENT: BlockId = BlockId(1);
const LIST: BlockId = BlockId(2);
const FIRST_ITEM: BlockId = BlockId(3);
const FIRST_PARAGRAPH: BlockId = BlockId(4);
const QUOTE: BlockId = BlockId(5);
const QUOTE_PARAGRAPH: BlockId = BlockId(6);
const FENCE: BlockId = BlockId(7);
const SECOND_ITEM: BlockId = BlockId(8);
const SECOND_PARAGRAPH: BlockId = BlockId(9);

fn enter(block: BlockId, kind: GreenKind) -> GreenToken {
    GreenToken::enter(block, kind, ClosedChildSummary::default())
}

fn property(tag: PropertyTag, bytes: &[u8]) -> GreenToken {
    GreenToken::Property(GreenProperty::new(tag, bytes).expect("small property"))
}

struct FixtureTokenBuilder<'a> {
    source: &'a str,
    cursor: usize,
    tokens: Vec<GreenToken>,
}

impl<'a> FixtureTokenBuilder<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            source,
            cursor: 0,
            tokens: Vec::new(),
        }
    }

    fn push(&mut self, token: GreenToken) {
        self.tokens.push(token);
    }

    fn coverage(&mut self, spelling: &str, owner_relative_depth: u32, part: CoveragePart) {
        let end = self
            .cursor
            .checked_add(spelling.len())
            .expect("fixture cursor");
        assert_eq!(&self.source[self.cursor..end], spelling);
        self.tokens.push(GreenToken::Coverage(
            CoverageAtom::new(
                spelling.len() as u64,
                spelling.encode_utf16().count() as u64,
                owner_relative_depth,
                part,
            )
            .expect("nonempty coverage"),
        ));
        self.cursor = end;
    }

    fn finish(self) -> Vec<GreenToken> {
        assert_eq!(self.cursor, self.source.len(), "total source ownership");
        self.tokens
    }
}

/// Storage/composition probe only: the spellings stand in for facts produced
/// by the future recursive block controller. Every source byte has one owner;
/// Enter/Exit structure is generic and never names a List-containing-Quote or
/// List-containing-Fence combination.
fn commonmark_321_tokens(
    a: &str,
    b: &str,
    c: &str,
    d: &str,
    eol: &str,
) -> (String, Vec<GreenToken>) {
    let source = format!("- {a}{eol}  > {b}{eol}  ```{eol}  {c}{eol}  ```{eol}- {d}{eol}");
    let mut output = FixtureTokenBuilder::new(&source);
    output.push(enter(DOCUMENT, GreenKind::DOCUMENT));
    output.push(enter(LIST, GreenKind::LIST));
    output.push(property(PropertyTag::LIST, &[0, b'-', 1, 1]));

    output.push(enter(FIRST_ITEM, GreenKind::ITEM));
    output.push(property(PropertyTag::ITEM, &[0, 2]));
    output.coverage("- ", 0, CoveragePart::BLOCK_MARKER);
    output.push(enter(FIRST_PARAGRAPH, GreenKind::PARAGRAPH));
    output.coverage(a, 0, CoveragePart::CONTENT);
    output.coverage(eol, 0, CoveragePart::GAP);
    output.push(GreenToken::Exit);

    output.push(enter(QUOTE, GreenKind::BLOCK_QUOTE));
    output.coverage("  ", 1, CoveragePart::CONTAINER_MARKER);
    output.coverage("> ", 0, CoveragePart::CONTAINER_MARKER);
    output.push(enter(QUOTE_PARAGRAPH, GreenKind::PARAGRAPH));
    output.coverage(b, 0, CoveragePart::CONTENT);
    output.coverage(eol, 0, CoveragePart::GAP);
    output.push(GreenToken::Exit);
    output.push(GreenToken::Exit);

    output.push(enter(FENCE, GreenKind::CODE_BLOCK));
    output.push(property(PropertyTag::FENCE, &[b'`', 3, 0]));
    output.coverage("  ", 1, CoveragePart::CONTAINER_MARKER);
    output.coverage("```", 0, CoveragePart::BLOCK_MARKER);
    output.coverage(eol, 0, CoveragePart::GAP);
    output.coverage("  ", 1, CoveragePart::CONTAINER_MARKER);
    output.coverage(c, 0, CoveragePart::CONTENT);
    output.coverage(eol, 0, CoveragePart::GAP);
    output.coverage("  ", 1, CoveragePart::CONTAINER_MARKER);
    output.coverage("```", 0, CoveragePart::BLOCK_MARKER);
    output.coverage(eol, 0, CoveragePart::GAP);
    output.push(GreenToken::Exit);
    output.push(GreenToken::Exit);

    output.push(enter(SECOND_ITEM, GreenKind::ITEM));
    output.push(property(PropertyTag::ITEM, &[1, 2]));
    output.coverage("- ", 0, CoveragePart::BLOCK_MARKER);
    output.push(enter(SECOND_PARAGRAPH, GreenKind::PARAGRAPH));
    output.coverage(d, 0, CoveragePart::CONTENT);
    output.coverage(eol, 0, CoveragePart::GAP);
    output.push(GreenToken::Exit);
    output.push(GreenToken::Exit);
    output.push(GreenToken::Exit);
    output.push(GreenToken::Exit);
    let tokens = output.finish();
    (source, tokens)
}

fn green(source: &str, tokens: Vec<GreenToken>) -> SerializedGreenSequence {
    let mut receipt = GreenMutationReceipt::default();
    let sequence = SerializedGreenSequence::from_tokens(tokens, &mut receipt).expect("green tree");
    assert_eq!(sequence.metric().bytes, source.len() as u64);
    assert_eq!(
        sequence.metric().utf16,
        source.encode_utf16().count() as u64
    );
    sequence
}

fn parse_m11(source: &str, fuel: usize) -> flark_parser::M11CleanDocumentResult {
    let store = SourceStore::new(source).expect("source");
    let mut job = M11CleanParseJob::new(store.snapshot()).expect("parse job");
    loop {
        match job.poll(fuel).expect("bounded exact-clean poll") {
            M11CleanParsePoll::Pending { transitions } => {
                assert!((1..=fuel).contains(&transitions));
            }
            M11CleanParsePoll::Complete {
                transitions,
                result,
            } => {
                assert!((1..=fuel).contains(&transitions));
                return result;
            }
        }
    }
}

fn reconstruct(sequence: &SerializedGreenSequence, source: &str) -> Vec<u8> {
    let mut offset = 0_u64;
    let mut output = Vec::with_capacity(source.len());
    while offset < source.len() as u64 {
        let hit = sequence
            .source_lookup(SourceCoordinate::Bytes, offset, Affinity::Downstream)
            .expect("covered source byte");
        assert_eq!(hit.byte_range.start, offset);
        let start = usize::try_from(hit.byte_range.start).expect("small source");
        let end = usize::try_from(hit.byte_range.end).expect("small source");
        output.extend_from_slice(&source.as_bytes()[start..end]);
        offset = hit.byte_range.end;
    }
    output
}

#[test]
fn commonmark_321_is_typed_fail_closed_by_m11_before_recursive_admission() {
    for fuel in [1, 2, 7, 31] {
        let result = parse_m11(COMMONMARK_321, fuel);
        assert_eq!(
            result.kind(),
            M11CleanDocumentKind::Unknown(M11UnknownReason::UnsupportedList(
                M11ListUnsupportedReason::BlockChild,
            )),
            "fuel={fuel}"
        );
        assert_eq!(result.source_range(), 0..COMMONMARK_321.len() as u32);
        let [leaf] = result.leaves() else {
            panic!("unsupported source must remain one exact coverage leaf");
        };
        assert_eq!(leaf.source_range(), 0..COMMONMARK_321.len() as u32);
        assert_eq!(leaf.source_utf16_range(), 0..COMMONMARK_321.len() as u32);
    }

    let nested = parse_m11(COMMONMARK_325, 1);
    assert!(matches!(
        nested.kind(),
        M11CleanDocumentKind::Unknown(M11UnknownReason::UnsupportedList(
            M11ListUnsupportedReason::Nested | M11ListUnsupportedReason::Loose
        ))
    ));
}

#[test]
fn generic_green_fits_commonmark_321_without_combination_specific_nodes() {
    assert_eq!(
        markdown_to_html(COMMONMARK_321, &ComrakOptions::default()),
        concat!(
            "<ul>\n<li>a\n<blockquote>\n<p>b</p>\n</blockquote>\n",
            "<pre><code>c\n</code></pre>\n</li>\n<li>d</li>\n</ul>\n"
        )
    );
    let (source, tokens) = commonmark_321_tokens("a", "b", "c", "d", "\n");
    assert_eq!(source, COMMONMARK_321);
    let sequence = green(&source, tokens);
    assert_eq!(reconstruct(&sequence, &source), source.as_bytes());

    let b = sequence
        .source_lookup(
            SourceCoordinate::Bytes,
            source.find('b').expect("b") as u64,
            Affinity::Downstream,
        )
        .expect("quote content");
    assert_eq!(
        b.enclosing,
        vec![DOCUMENT, LIST, FIRST_ITEM, QUOTE, QUOTE_PARAGRAPH]
    );

    let c_offset = source.find("  c").expect("code body") + 2;
    let c = sequence
        .source_lookup(
            SourceCoordinate::Bytes,
            c_offset as u64,
            Affinity::Downstream,
        )
        .expect("fence body");
    assert_eq!(c.enclosing, vec![DOCUMENT, LIST, FIRST_ITEM, FENCE]);
    let code_span = sequence.block_span_from_hit(&c, 3).expect("code hull");
    assert_eq!(code_span.byte_range, 10..26);

    let parent_owned_indent = sequence
        .source_lookup(
            SourceCoordinate::Bytes,
            (c_offset - 1) as u64,
            Affinity::Downstream,
        )
        .expect("list continuation indentation");
    assert_eq!(parent_owned_indent.owner, FIRST_ITEM);
    assert_eq!(
        parent_owned_indent.open_path,
        vec![DOCUMENT, LIST, FIRST_ITEM, FENCE]
    );

    let d = sequence
        .source_lookup(
            SourceCoordinate::Bytes,
            source.rfind('d').expect("d") as u64,
            Affinity::Downstream,
        )
        .expect("second item");
    assert_eq!(
        d.enclosing,
        vec![DOCUMENT, LIST, SECOND_ITEM, SECOND_PARAGRAPH]
    );
    let list_span = sequence.block_span_from_hit(&d, 1).expect("list hull");
    assert_eq!(list_span.byte_range, 0..30);
    assert!(
        sequence
            .direct_child_summary(&list_span, &mut Default::default())
            .expect("list child fold")
            .list_is_tight()
    );
}

#[test]
fn recursive_coverage_keeps_byte_and_utf16_axes_exact() {
    let (source, tokens) = commonmark_321_tokens("α", "🧪", "λ", "δ", "\r\n");
    let sequence = green(&source, tokens);
    assert_eq!(reconstruct(&sequence, &source), source.as_bytes());

    let byte_offset = source.find("🧪").expect("unicode quote body");
    let utf16_offset = source[..byte_offset].encode_utf16().count();
    let byte_hit = sequence
        .source_lookup(
            SourceCoordinate::Bytes,
            byte_offset as u64,
            Affinity::Downstream,
        )
        .expect("byte lookup");
    let utf16_hit = sequence
        .source_lookup(
            SourceCoordinate::Utf16,
            utf16_offset as u64,
            Affinity::Downstream,
        )
        .expect("UTF-16 lookup");
    assert_eq!(byte_hit.owner, QUOTE_PARAGRAPH);
    assert_eq!(utf16_hit.owner, QUOTE_PARAGRAPH);
    assert_eq!(
        byte_hit.byte_range,
        byte_offset as u64..byte_offset as u64 + 4
    );
    assert_eq!(
        utf16_hit.utf16_range,
        utf16_offset as u64..utf16_offset as u64 + 2
    );
}

fn simple_item_tokens(item: u64, paragraph: u64) -> [GreenToken; 6] {
    [
        enter(BlockId(item), GreenKind::ITEM),
        property(PropertyTag::ITEM, &[0, 2]),
        GreenToken::Coverage(
            CoverageAtom::new(2, 2, 0, CoveragePart::BLOCK_MARKER).expect("marker"),
        ),
        enter(BlockId(paragraph), GreenKind::PARAGRAPH),
        GreenToken::Coverage(CoverageAtom::new(2, 2, 0, CoveragePart::CONTENT).expect("content")),
        GreenToken::Exit,
    ]
}

#[test]
fn length_change_updates_only_a_logarithmic_route_in_a_large_list() {
    const ITEMS: u64 = 20_000;
    const TARGET: u64 = ITEMS / 2;
    let mut tokens = Vec::with_capacity((ITEMS as usize) * 7 + 4);
    tokens.push(enter(DOCUMENT, GreenKind::DOCUMENT));
    tokens.push(enter(LIST, GreenKind::LIST));
    tokens.push(property(PropertyTag::LIST, &[0, b'-', 1, 1]));
    for index in 0..ITEMS {
        let item = 100 + index * 2;
        tokens.extend(simple_item_tokens(item, item + 1));
        tokens.push(GreenToken::Exit);
    }
    tokens.push(GreenToken::Exit);
    tokens.push(GreenToken::Exit);

    let mut build = GreenMutationReceipt::default();
    let sequence = SerializedGreenSequence::from_tokens(tokens, &mut build).expect("large list");
    let target_content_offset = TARGET * 4 + 2;
    let target = sequence
        .source_lookup(
            SourceCoordinate::Bytes,
            target_content_offset,
            Affinity::Downstream,
        )
        .expect("target item content");
    let suffix_before = sequence
        .source_lookup(
            SourceCoordinate::Bytes,
            (ITEMS - 1) * 4 + 2,
            Affinity::Downstream,
        )
        .expect("suffix content");

    let mut mutation = GreenMutationReceipt::default();
    let revised = sequence
        .replace_token(
            &target.cursor,
            GreenToken::Coverage(
                CoverageAtom::new(9, 7, 0, CoveragePart::CONTENT).expect("replacement metric"),
            ),
            &mut mutation,
        )
        .expect("local metric replacement");

    assert_eq!(revised.metric().bytes, sequence.metric().bytes + 7);
    assert_eq!(revised.metric().utf16, sequence.metric().utf16 + 5);
    assert!(mutation.nodes_visited < 64, "{mutation:?}");
    assert!(mutation.nodes_allocated < 64, "{mutation:?}");
    assert_eq!(mutation.leaf_pages_allocated, 1, "{mutation:?}");
    let suffix_after = revised
        .source_lookup(
            SourceCoordinate::Bytes,
            suffix_before.byte_range.start + 7,
            Affinity::Downstream,
        )
        .expect("shifted suffix content");
    assert_eq!(suffix_after.owner, suffix_before.owner);
    assert_eq!(
        suffix_after.byte_range.start,
        suffix_before.byte_range.start + 7
    );
    assert!(matches!(
        revised.validate_cursor(&target.cursor),
        Err(euler_structure_challenger::serialized_green::GreenError::StaleCursor)
    ));
    eprintln!(
        "commonmark_321_large_list items={ITEMS} build_nodes={} build_pages={} mutation_nodes_visited={} mutation_nodes_allocated={} mutation_pages={} suffix_shift_bytes=7",
        build.nodes_allocated,
        build.leaf_pages_allocated,
        mutation.nodes_visited,
        mutation.nodes_allocated,
        mutation.leaf_pages_allocated,
    );
}
