use comrak::nodes::{ListDelimType, ListType, NodeList, NodeValue};
use comrak::{markdown_to_html, parse_document, Arena, Options};

#[test]
fn start_ordinal_delimiter_and_digit_limit_match_comrak() {
    let cases = [
        (
            "0. zero\n1. one\n",
            "<ol start=\"0\">\n<li>zero</li>\n<li>one</li>\n</ol>\n",
        ),
        (
            "2. two\n99. source ordinal ignored\n",
            "<ol start=\"2\">\n<li>two</li>\n<li>source ordinal ignored</li>\n</ol>\n",
        ),
        (
            "3) three\n4) four\n",
            "<ol start=\"3\">\n<li>three</li>\n<li>four</li>\n</ol>\n",
        ),
        (
            "001. one\n002. two\n",
            "<ol>\n<li>one</li>\n<li>two</li>\n</ol>\n",
        ),
        (
            "1. period\n2) paren\n",
            "<ol>\n<li>period</li>\n</ol>\n<ol start=\"2\">\n<li>paren</li>\n</ol>\n",
        ),
        (
            "123456789. item\n",
            "<ol start=\"123456789\">\n<li>item</li>\n</ol>\n",
        ),
        ("1234567890. item\n", "<p>1234567890. item</p>\n"),
        ("١. not ASCII\n", "<p>١. not ASCII</p>\n"),
        ("1.item\n", "<p>1.item</p>\n"),
    ];
    for (source, expected) in cases {
        assert_eq!(
            markdown_to_html(source, &Options::default()),
            expected,
            "{source:?}"
        );
    }
}

#[test]
fn marker_width_indent_and_padding_metadata_match_comrak() {
    let source = "12)  α\r\n003) beta\r\n123456789)    wide\r\n";
    let arena = Arena::new();
    let root = parse_document(&arena, source, &Options::default());
    let lists = root
        .descendants()
        .filter_map(|node| match &node.data().value {
            NodeValue::List(list) => Some(*list),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        lists,
        vec![NodeList {
            list_type: ListType::Ordered,
            marker_offset: 0,
            padding: 5,
            start: 12,
            delimiter: ListDelimType::Paren,
            bullet_char: 0,
            tight: true,
            is_task_list: false,
        }]
    );
    let items = root
        .descendants()
        .filter_map(|node| match &node.data().value {
            NodeValue::Item(item) => Some(*item),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(items.len(), 3);
    assert_eq!(
        items.iter().map(|item| item.padding).collect::<Vec<_>>(),
        [5, 5, 14]
    );
    assert!(items.iter().all(|item| {
        item.list_type == ListType::Ordered
            && item.delimiter == ListDelimType::Paren
            && item.marker_offset == 0
    }));

    let indented_arena = Arena::new();
    let indented = parse_document(&indented_arena, "  12)  α\r\n", &Options::default());
    let item = indented
        .descendants()
        .find_map(|node| match &node.data().value {
            NodeValue::Item(item) => Some(*item),
            _ => None,
        })
        .expect("indented ordered item");
    assert_eq!(item.marker_offset, 2);
    assert_eq!(item.padding, 5);

    assert_eq!(
        markdown_to_html("1.     code\n", &Options::default()),
        "<ol>\n<li>\n<pre><code>code\n</code></pre>\n</li>\n</ol>\n",
        "five post-marker spaces fall back to one structural space and a code child"
    );
}

#[test]
fn interruption_and_terminal_empty_rules_match_comrak() {
    let cases = [
        ("paragraph\n2. item\n", "<p>paragraph\n2. item</p>\n"),
        (
            "paragraph\n1. item\n",
            "<p>paragraph</p>\n<ol>\n<li>item</li>\n</ol>\n",
        ),
        (
            "paragraph\n01) item\n",
            "<p>paragraph</p>\n<ol>\n<li>item</li>\n</ol>\n",
        ),
        ("paragraph\n1.\n", "<p>paragraph\n1.</p>\n"),
        ("1. first\n2.\n", "<ol>\n<li>first</li>\n<li></li>\n</ol>\n"),
        (
            "1. first\r\n2.\r\n",
            "<ol>\n<li>first</li>\n<li></li>\n</ol>\n",
        ),
        ("1. first\n2. ", "<ol>\n<li>first</li>\n<li></li>\n</ol>\n"),
        ("1. first\n2.", "<ol>\n<li>first\n2.</li>\n</ol>\n"),
        ("1.\n", "<ol>\n<li></li>\n</ol>\n"),
    ];
    for (source, expected) in cases {
        assert_eq!(
            markdown_to_html(source, &Options::default()),
            expected,
            "{source:?}"
        );
    }
}

#[test]
fn excluded_v1_shapes_have_distinct_commonmark_container_semantics() {
    let cases = [
        (
            "1. first\n\n2. second\n",
            "<ol>\n<li>\n<p>first</p>\n</li>\n<li>\n<p>second</p>\n</li>\n</ol>\n",
        ),
        (
            "1. outer\n   1. inner\n",
            "<ol>\n<li>outer\n<ol>\n<li>inner</li>\n</ol>\n</li>\n</ol>\n",
        ),
        (
            "1. first\n   continuation\n",
            "<ol>\n<li>first\ncontinuation</li>\n</ol>\n",
        ),
        (
            "1. first\n2.\n3. third\n",
            "<ol>\n<li>first</li>\n<li></li>\n<li>third</li>\n</ol>\n",
        ),
    ];
    for (source, expected) in cases {
        assert_eq!(
            markdown_to_html(source, &Options::default()),
            expected,
            "{source:?}"
        );
    }

    let mut task_options = Options::default();
    task_options.extension.tasklist = true;
    assert_eq!(
        markdown_to_html("1. [x] task\n", &task_options),
        "<ol>\n<li><input type=\"checkbox\" checked=\"\" disabled=\"\" /> task</li>\n</ol>\n"
    );
}
