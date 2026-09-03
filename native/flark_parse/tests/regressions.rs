//! Cases from the M1 review, each pinned by the behavior that was wrong.
mod common;
use common::check_invariants;
use flark_parse::model::Extractor;
use flark_parse::schema::{self, block, block_kind, content, definition, header, run, run_kind};

struct M { w: Vec<u32>, devs: Vec<String> }
impl M {
    fn of(src: &str) -> M { let (w, d) = Extractor::extract_with_report(src); check_invariants(src, &w).unwrap_or_else(|e| panic!("{e} for {src:?}")); M { w, devs: d.iter().map(|x| format!("{} {}", x.rule, x.detail)).collect() } }
    fn n(&self, f: usize) -> usize { self.w[f] as usize }
    fn blocks_off(&self) -> usize { schema::HEADER_WORDS + self.n(header::LINE_COUNT) * 2 }
    fn content_off(&self) -> usize { self.blocks_off() + self.n(header::BLOCK_COUNT) * block::WORDS }
    fn runs_off(&self) -> usize { self.content_off() + self.n(header::CONTENT_COUNT) * content::WORDS }
    fn defs_off(&self) -> usize { self.runs_off() + self.n(header::RUN_COUNT) * run::WORDS }
    fn block(&self, i: usize, f: usize) -> usize { self.w[self.blocks_off() + i * block::WORDS + f] as usize }
    fn content(&self, i: usize, f: usize) -> usize { self.w[self.content_off() + i * content::WORDS + f] as usize }
    fn run(&self, i: usize, f: usize) -> usize { self.w[self.runs_off() + i * run::WORDS + f] as usize }
    fn def(&self, i: usize, f: usize) -> usize { self.w[self.defs_off() + i * definition::WORDS + f] as usize }
    fn contents<'a>(&self, src: &'a str) -> Vec<&'a str> { (0..self.n(header::CONTENT_COUNT)).map(|i| &src[self.content(i, content::START_BYTE)..self.content(i, content::END_BYTE)]).collect() }
    fn run_contents<'a>(&self, src: &'a str) -> Vec<(usize, &'a str)> { (0..self.n(header::RUN_COUNT)).map(|i| (self.run(i, run::KIND), &src[self.run(i, run::CONTENT_START_BYTE)..self.run(i, run::CONTENT_END_BYTE)])).collect() }
    fn defs<'a>(&self, src: &'a str) -> Vec<&'a str> { (0..self.n(header::DEFINITION_COUNT)).map(|i| &src[self.def(i, definition::START_BYTE)..self.def(i, definition::END_BYTE)]).collect() }
    fn clean(&self) { assert!(self.devs.is_empty(), "deviations: {:?}", self.devs); }
}

#[test]
fn bare_carriage_returns_are_line_endings() {
    let src = "a\rb\r*c*";
    let m = M::of(src); m.clean();
    assert_eq!(m.contents(src), ["a", "b", "*c*"]);
    assert_eq!(m.run_contents(src).iter().filter(|(k, _)| *k == run_kind::EMPH as usize).map(|(_, t)| *t).collect::<Vec<_>>(), ["c"]);
    let m = M::of("é\rb*x*"); m.clean();
    let m = M::of("[foo]: /url\r[foo]"); m.clean(); assert_eq!(m.n(header::DEFINITION_COUNT), 1);
}

#[test]
fn crlf_is_not_part_of_code_or_html_content() {
    let src = "```\ncode\r\n```\n";
    let m = M::of(src); m.clean();
    assert_eq!(m.contents(src), ["code"]);
    let src = "<div>\r\nx\r\n</div>\r\n";
    let m = M::of(src); m.clean();
    assert_eq!(m.contents(src), ["<div>", "x", "</div>"]);
}

#[test]
fn empty_document_is_a_lone_document_block() {
    let m = M::of(""); m.clean();
    assert_eq!(m.n(header::BLOCK_COUNT), 1);
}

#[test]
fn unreferenced_footnote_definitions_keep_their_place() {
    let src = "[^a]: note text\n\npara";
    let m = M::of(src); m.clean();
    assert_eq!(m.block(1, block::KIND), block_kind::FOOTNOTE_DEFINITION as usize);
    assert_eq!(m.contents(src), ["note text", "para"]);
    let src = "[^1]: note\n\n[^1]";
    let m = M::of(src); m.clean();
    assert!(m.block(1, block::START_BYTE) < m.block(3, block::START_BYTE), "document order");
}

#[test]
fn definitions_inside_containers_and_across_lines_are_recorded() {
    for (src, expected) in [
        ("> [foo]: /url\n\n[foo]", vec!["[foo]: /url\n"]),
        ("- [foo]: /url\n\n[foo]", vec!["[foo]: /url\n"]),
        ("[foo]:\n/url\n\n[foo]", vec!["[foo]:\n/url\n"]),
        ("[foo]: /url '\ntitle\nline1\nline2\n'\n\n[foo]", vec!["[foo]: /url '\ntitle\nline1\nline2\n'\n"]),
        ("[a\\]b]: /url\n\n[a\\]b]", vec!["[a\\]b]: /url\n"]),
        ("> [foo]: /url\n> bar", vec!["[foo]: /url\n"]),
        ("[\nfoo]: /url\nbar", vec!["[\nfoo]: /url\n"]),
        ("[foo]: <bar>", vec!["[foo]: <bar>"]),
    ] {
        let m = M::of(src); m.clean();
        assert_eq!(m.defs(src), expected, "for {src:?}");
    }
    let src = "> [foo]: /url\n> bar";
    let m = M::of(src);
    assert_eq!(m.contents(src), ["bar"]);
}

#[test]
fn task_items_start_content_after_the_checkbox() {
    let src = "- [x]  foo\n  bar";
    let m = M::of(src); m.clean();
    assert_eq!(m.contents(src), [" foo", "bar"]);
    assert_eq!(m.block(2, block::ATTR0), 2, "container offset is the list padding");
    let src = "- [ ] foo\n\n  > quote";
    let m = M::of(src); m.clean();
    assert_eq!(m.contents(src), ["foo", "quote"]);
}

#[test]
fn nested_markers_consume_their_padding() {
    for src in ["> -     code\n", "- -     code\n", "> 1.     code\n"] {
        let m = M::of(src); m.clean();
        assert_eq!(m.contents(src), ["code"], "for {src:?}");
    }
}

#[test]
fn lazy_continuation_after_a_stripped_definition() {
    for src in ["- [a]: /u\n [a]", "- [é]: /u\n [é]", "[foo]: /url\nbar\n===\n[foo]\n"] {
        let m = M::of(src); m.clean();
    }
    let src = "- [a]: /u\n [a]";
    let m = M::of(src);
    assert_eq!(m.run_contents(src).iter().find(|(k, _)| *k == run_kind::LINK as usize).map(|(_, t)| *t), Some("a"));
}

#[test]
fn escaped_pipes_in_cells_shift_correctly() {
    for src in ["| a |\n|---|\n| \\|*x* |\n", "| a |\n|---|\n| é\\|*x* |\n", "| a |\n|---|\n|x\\\\|y `z\\\\|w`|\n", "| a | b |\n|---|---|\n| `\\|` | **\\|** |\n"] {
        let m = M::of(src); m.clean();
    }
    let src = "| a |\n|---|\n| \\|*x* |\n";
    let m = M::of(src);
    assert!(m.run_contents(src).iter().any(|(k, t)| *k == run_kind::EMPH as usize && *t == "x"), "{:?}", m.run_contents(src));
}

#[test]
fn link_destinations_honor_escapes_tabs_and_angle_brackets() {
    let src = "[a](foo\\)) [b](\turl) [c](<u v> \"t\\\"q\")";
    let m = M::of(src); m.clean();
    let links: Vec<(usize, usize, usize, usize)> = (0..m.n(header::RUN_COUNT)).filter(|i| m.run(*i, run::KIND) == run_kind::LINK as usize).map(|i| (m.run(i, run::AUX0), m.run(i, run::AUX1), m.run(i, run::AUX2), m.run(i, run::AUX3))).collect();
    assert_eq!(&src[links[0].0..links[0].1], "foo\\)");
    assert_eq!(&src[links[1].0..links[1].1], "url");
    assert_eq!(&src[links[2].0..links[2].1], "u v");
    assert_eq!(&src[links[2].2..links[2].3], "t\\\"q");
}

#[test]
fn a_wide_table_reports_the_alignment_cap() {
    let header: String = (0..17).map(|_| "| a ").collect::<String>() + "|\n";
    let delim: String = (0..16).map(|_| "|---").collect::<String>() + "|-:|\n";
    let row: String = (0..17).map(|_| "| 1 ").collect::<String>() + "|\n";
    let src = header + &delim + &row;
    let m = M::of(&src);
    assert!(m.devs.iter().any(|d| d.starts_with("table-alignment-cap")), "{:?}", m.devs);
}

#[test]
fn crlf_documents_extract_exactly() {
    let src = "# Title\r\n\r\n- one *em*\r\n- two\r\n\r\n> quote\r\ncontinued\r\n\r\n[a]: /u\r\n\r\n[a] and `code\r\nspan`\r\n";
    let m = M::of(src); m.clean();
    assert_eq!(m.contents(src), ["Title", "one *em*", "two", "quote", "continued", "[a] and `code", "span`"]);
    assert_eq!(m.defs(src), ["[a]: /u\r\n"]);
}

#[test]
fn crlf_after_an_escaped_bracket_keeps_positions() {
    for src in ["1. -~\\[\r\n---:\r\nbar*", " a\\\"t\"[^1][^1]: nb---1. -~\\[\r\n---:\r\nbar*", "x\\[\r\nbar", "x\\[\nbar"] {
        let m = M::of(src);
        eprintln!("{src:?}: contents {:?} runs {:?} devs {:?}", m.contents(src), m.run_contents(src), m.devs);
        m.clean();
    }
}

#[test]
fn a_checkbox_after_a_stripped_definition_is_found_in_the_source() {
    let src = "1. [a]: /u\n\t[ ]";
    let m = M::of(src); m.clean();
    assert_eq!(m.defs(src), ["[a]: /u\n"]);
    assert_eq!(m.block(2, block::FLAGS) & 1, 1, "task item");
    assert_eq!(&src[m.block(2, block::ATTR1) - 1..m.block(2, block::ATTR2) + 1], "[ ]");
}

#[test]
fn a_paragraph_split_by_a_table_header_keeps_its_definition_text() {
    let src = "[a]: /u\nfoo\nhdr\n:---\n";
    let m = M::of(src); m.clean();
    assert_eq!(m.contents(src), ["[a]: /u", "foo", "hdr"]);
    assert_eq!(m.defs(src), Vec::<&str>::new());
}
