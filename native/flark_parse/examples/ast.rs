use comrak::nodes::NodeValue;
use comrak::{parse_document, Arena};
use flark_parse::model::options;
fn walk<'a>(n: &'a comrak::nodes::AstNode<'a>, depth: usize) {
    let d = n.data.borrow();
    let label = match &d.value { NodeValue::Text(t) => format!("Text({:?})", t), NodeValue::Link(l) => format!("Link({:?})", l.url), v => format!("{:?}", v).split('(').next().unwrap_or("?").to_string() };
    println!("{}{} @ {}", "  ".repeat(depth), label, d.sourcepos);
    drop(d);
    for c in n.children() { walk(c, depth + 1); }
}
fn main() {
    for arg in std::env::args().skip(1) {
        // `@path` reads the Markdown from a file; otherwise \n, \r, \t escapes are expanded.
        let src = if let Some(path) = arg.strip_prefix('@') { std::fs::read_to_string(path).expect("read") } else { arg.replace("\\n", "\n").replace("\\r", "\r").replace("\\t", "\t") };
        println!("---- {:?}", src);
        let arena = Arena::new();
        walk(parse_document(&arena, &src, &options()), 0);
        println!("html: {:?}", comrak::markdown_to_html(&src, &options()));
    }
}
