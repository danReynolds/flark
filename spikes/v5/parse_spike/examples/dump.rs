use comrak::nodes::NodeValue;
use comrak::{parse_document, Arena};
use flark_parse_spike::model::options;
fn walk<'a>(n: &'a comrak::nodes::AstNode<'a>, depth: usize, src: &str) {
    let d = n.data.borrow();
    let sp = d.sourcepos;
    let label = match &d.value {
        NodeValue::Text(t) => format!("Text({:?})", t),
        NodeValue::Link(l) => format!("Link(url={:?}, title={:?})", l.url, l.title),
        NodeValue::TaskItem(t) => format!("TaskItem(symbol={:?}, symbol_sourcepos={})", t.symbol, t.symbol_sourcepos),
        NodeValue::Heading(h) => format!("Heading(level={}, setext={})", h.level, h.setext),
        NodeValue::TableCell => "TableCell".into(),
        NodeValue::TableRow(h) => format!("TableRow(header={h})"),
        NodeValue::Table(t) => format!("Table(align={:?})", t.alignments),
        NodeValue::Item(l) => format!("Item(offset={}, padding={})", l.marker_offset, l.padding),
        NodeValue::List(l) => format!("List({:?} start={} tight={})", l.list_type, l.start, l.tight),
        NodeValue::CodeBlock(c) => format!("CodeBlock(fenced={} len={} off={} info={:?})", c.fenced, c.fence_length, c.fence_offset, c.info),
        NodeValue::FootnoteDefinition(f) => format!("FootnoteDef({:?})", f.name),
        NodeValue::FootnoteReference(f) => format!("FootnoteRef({:?})", f.name),
        NodeValue::Image(l) => format!("Image(url={:?})", l.url),
        other => format!("{:?}", std::mem::discriminant(other)).replace("Discriminant(", "").replace(")", ""),
    };
    let _ = src;
    println!("{}{} @ {}", "  ".repeat(depth), label, sp);
    drop(d);
    for c in n.children() { walk(c, depth + 1, src); }
}
fn main() {
    let cases = [
        "| a | b |\n|:--|--:|\n| *1* | 2 \\| 3 |\n",
        "- [x] done\n- [ ] todo\n",
        "Title\n=====\n\n## Closed ##\n",
        "[text](http://x.y \"the title\") and ![alt](i.png)\n\n[foo]: /url\n\nsee [foo]\n",
        "***\n\n<div>\nhtml\n</div>\n\nHere[^1]\n\n[^1]: note\n",
        "> quote\ncontinued lazily\n\n1. one\n\n   two\n",
    ];
    for c in cases {
        println!("---- {:?}", c);
        let arena = Arena::new();
        let root = parse_document(&arena, c, &options());
        walk(root, 0, c);
    }
}
