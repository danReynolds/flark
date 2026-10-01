//! Cases from the M1 review, each pinned by the behavior that was wrong.
mod common;
use common::check_invariants;
use flark_parse::model::Extractor;
use flark_parse::records::{self, block, content, definition, expand, header, run};
use flark_parse::schema::{block_kind, run_kind};

/// A published model expanded into the internal layout, so assertions can
/// name UTF-8 byte ranges of the source as well as UTF-16 offsets.
struct M { w: Vec<u32>, devs: Vec<String> }
impl M {
    fn of(src: &str) -> M { let (w, d) = Extractor::extract_with_report(src); check_invariants(src, &w).unwrap_or_else(|e| panic!("{e} for {src:?}")); M { w: expand(src, &w).unwrap(), devs: d.iter().map(|x| format!("{} {}", x.rule, x.detail)).collect() } }
    fn n(&self, f: usize) -> usize { self.w[f] as usize }
    fn blocks_off(&self) -> usize { records::HEADER_WORDS + self.n(header::LINE_COUNT) * 2 }
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
    fn string(&self, offset: usize, length: usize) -> String {
        let words = &self.w[self.defs_off() + self.n(header::DEFINITION_COUNT) * definition::WORDS..];
        let bytes: Vec<u8> = words.iter().flat_map(|x| x.to_le_bytes()).collect();
        String::from_utf8(bytes[offset..offset + length].to_vec()).unwrap()
    }
    /// Block [b]'s text as a host shows it: the content, or the display text
    /// of a replacement, of each run without children, and a line break for
    /// each break run. The invariants put every other content byte in a run.
    fn shown(&self, src: &str, b: usize) -> String {
        let runs: Vec<usize> = (0..self.n(header::RUN_COUNT)).filter(|&r| self.run(r, run::BLOCK) == b).collect();
        runs.iter().filter(|&&r| !runs.iter().any(|&c| self.run(c, run::PARENT) == r)).map(|&r| match self.run(r, run::KIND) as u32 {
            run_kind::REPLACEMENT => self.string(self.run(r, run::AUX0), self.run(r, run::AUX1)),
            run_kind::SOFT_BREAK | run_kind::HARD_BREAK => "\n".to_string(),
            _ => src[self.run(r, run::CONTENT_START_BYTE)..self.run(r, run::CONTENT_END_BYTE)].to_string(),
        }).collect()
    }
    fn blocks_of(&self, kind: u32) -> Vec<usize> { (0..self.n(header::BLOCK_COUNT)).filter(|&b| self.block(b, block::KIND) == kind as usize).collect() }
}

/// A model `extract` would publish (fail-safe degradations allowed), checked
/// against the invariants.
fn published(src: &str) -> Vec<String> {
    let (w, devs) = Extractor::extract_with_report(src);
    check_invariants(src, &w).unwrap_or_else(|e| panic!("{e} for {src:?}"));
    assert!(devs.iter().all(|d| d.leaf.is_some()), "refused {src:?}: {:?}", devs.iter().map(|d| (d.rule, &d.detail)).collect::<Vec<_>>());
    devs.iter().map(|d| d.rule.to_string()).collect()
}

#[test]
fn definition_buffer_index_preserves_multiline_unicode_and_container_ranges() {
    for prefix in ["", "> ", "> > "] {
        for newline in ["\n", "\r\n", "\r"] {
            for trailing_text in [false, true] {
                let mut src = String::new();
                let mut expected = Vec::new();
                for i in 0..128 {
                    let label = format!("référence{i}");
                    let dest = format!("/café/{i}");
                    src.push_str(prefix);
                    let start = src.len();
                    src.push_str(&format!("[{label}]:{newline}{prefix}  {dest} \"title\"{newline}"));
                    expected.push((start, src.len(), label, dest));
                }
                if trailing_text { src.push_str(&format!("{prefix}after **words**")); }
                let m = M::of(&src); m.clean();
                assert_eq!(m.n(header::DEFINITION_COUNT), expected.len());
                for (i, (start, end, label, dest)) in expected.iter().enumerate() {
                    assert_eq!(m.def(i, definition::START_BYTE), *start);
                    assert_eq!(m.def(i, definition::END_BYTE), *end);
                    assert_eq!(&src[m.def(i, definition::LABEL_START_BYTE)..m.def(i, definition::LABEL_END_BYTE)], label);
                    assert_eq!(&src[m.def(i, definition::DEST_START_BYTE)..m.def(i, definition::DEST_END_BYTE)], dest);
                }
            }
        }
    }
}

#[test]
fn empty_atx_heading_separator_is_prefix_not_content() {
    for level in 1..=6 {
      for outer in ["", "> ", "- ", "  "] {
       for separator in [" ", "  ", "\t", " \t"] {
        for ending in ["", "\n", "\r\n"] {
            let prefix = format!("{outer}{}{separator}", "#".repeat(level));
            let src = format!("{prefix}{ending}");
            let m = M::of(&src); m.clean();
            let heading = (0..m.n(header::BLOCK_COUNT)).find(|&b| m.block(b, block::KIND) == block_kind::HEADING as usize).unwrap();
            let c = m.block(heading, block::CONTENT_OFFSET);
            assert_eq!(m.content(c, content::START_BYTE), prefix.len(), "for {src:?}");
            assert_eq!(m.content(c, content::END_BYTE), prefix.len(), "for {src:?}");

            let typed = format!("{prefix}é  {ending}");
            let m = M::of(&typed); m.clean();
            assert_eq!(m.contents(&typed), ["é  "], "for {typed:?}");
        }
       }
      }
    }
}

#[test]
fn literal_tabs_do_not_hide_a_shifted_pipeless_cell() {
    for padding in ["", " ", "  ", "   "] {
      for body_padding in ["", " ", "  ", "   "] {
       for (first, next) in [("", ""), ("> ", "> "), ("- ", "  "), ("> - ", ">   ")] {
        for ending in ["\n", "\r\n"] {
          for (head, body, contents) in [("h", "**<\t\t**", vec!["h", "**<\t\t**"]), ("|h|j|", "|**<\t\t**|é|", vec!["h", "j", "**<\t\t**", "é"])] {
            let delim = if head == "h" { "--- |" } else { "|---|---|" };
            // Padding after a list marker belongs to the item prefix.
            let item_padding = if first.ends_with("- ") { padding } else { "" };
            let src = format!("{first}{padding}{head}{ending}{next}{padding}{delim}{ending}{next}{item_padding}{body_padding}{body}");
            let m = M::of(&src); m.clean();
            assert_eq!(m.contents(&src), contents, "for {src:?}");
            assert!((0..m.n(header::RUN_COUNT)).all(|r| m.run(r, run::KIND) == run_kind::TEXT as usize), "literal text must have exact source ranges for {src:?}: {:?}", m.run_contents(&src));
          }
        }
       }
      }
    }
}

#[test]
fn multiline_code_repair_updates_the_complete_owner_range() {
    let src = "#### **foo *bar***\n].\n `!\n***-<***b`-`";
    let m = M::of(src); m.clean();
    let code = (0..m.n(header::RUN_COUNT)).find(|&r| m.run(r, run::KIND) == run_kind::CODE as usize).unwrap();
    assert_eq!(m.run(code, run::END_BYTE), m.run(code, run::CONTENT_END_BYTE) + 1);
    assert_eq!(&src[m.run(code, run::END_BYTE)..], "-`");
}

#[test]
fn multiline_inline_link_closures_and_following_breaks_are_exact() {
    for marker in ["", "!"] {
        for ending in ["\n", "\r\n"] {
          for prefix in ["", "> "] {
           for suffix in ["", ")", ")) extra", "]"] {
            let src = format!("{prefix}{marker}[link](   /uri{ending}{prefix}  \"title\"  ){suffix}{ending}{prefix}next");
            let m = M::of(&src); m.clean();
            let kind = if marker.is_empty() { run_kind::LINK } else { run_kind::IMAGE };
            let owner = (0..m.n(header::RUN_COUNT)).find(|&r| m.run(r, run::KIND) == kind as usize).unwrap();
            let end = src.find(')').unwrap() + 1;
            assert_eq!(m.run(owner, run::END_BYTE), end, "for {src:?}");
            assert_eq!(&src[m.run(owner, run::AUX0)..m.run(owner, run::AUX1)], "/uri");
            assert_eq!(&src[m.run(owner, run::AUX2)..m.run(owner, run::AUX3)], "title");
            let br = (0..m.n(header::RUN_COUNT)).find(|&r| m.run(r, run::KIND) == run_kind::SOFT_BREAK as usize).unwrap();
            let break_start = end + suffix.len();
            assert_eq!((m.run(br, run::START_BYTE), m.run(br, run::END_BYTE)), (break_start, break_start + ending.len()), "for {src:?}");
           }
          }
        }
    }
}

#[test]
fn bare_url_ending_in_an_angle_has_no_hidden_closing_bracket() {
    for prefix in ["", "# ", "> "] {
        let src = format!("{prefix}< http://foo.ba&`>\n");
        let m = M::of(&src); m.clean();
        let r = (0..m.n(header::RUN_COUNT)).find(|&r| m.run(r, run::KIND) == run_kind::AUTOLINK as usize).unwrap();
        assert_eq!(m.run(r, run::START_BYTE), m.run(r, run::CONTENT_START_BYTE));
        assert_eq!(m.run(r, run::END_BYTE), m.run(r, run::CONTENT_END_BYTE));
    }
}

#[test]
fn editing_content_retains_trailing_whitespace() {
    for prefix in ["", "- ", "> ", "# ", "> - "] {
        for ending in ["", "\n", "\r\n"] {
            let src = format!("{prefix}alpha  {ending}");
            let m = M::of(&src); m.clean();
            assert_eq!(m.contents(&src), ["alpha  "], "for {src:?}");
        }
    }
    let src = "# alpha  #";
    let m = M::of(src); m.clean();
    assert_eq!(m.contents(src), ["alpha "]);
    for src in ["| alpha  |\n| --- |", "|alpha  |\n|---|"] {
        let m = M::of(src); m.clean();
        assert_eq!(m.contents(src), ["alpha  "], "for {src:?}");
    }
    let src = "alpha  \r\nnext";
    let m = M::of(src); m.clean();
    assert_eq!(m.contents(src), ["alpha  ", "next"]);
    let br = (0..m.n(header::RUN_COUNT)).find(|&r| m.run(r, run::KIND) == run_kind::HARD_BREAK as usize).unwrap();
    assert_eq!((m.run(br, run::START_BYTE), m.run(br, run::END_BYTE)), (5, 9));
    assert_eq!((m.run(br, run::CONTENT_START_BYTE), m.run(br, run::CONTENT_END_BYTE)), (5, 7));
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
    // Content is in block order: the item's own blank line precedes the
    // records of the paragraph and quote inside it.
    assert_eq!(m.contents(src), ["", "foo", "quote"]);
}

#[test]
fn empty_nested_quote_publishes_only_the_innermost_prefix() {
    let src = "> > a\n> > ";
    let m = M::of(src); m.clean();
    let c = m.block(2, block::CONTENT_OFFSET);
    assert_eq!(m.block(2, block::CONTENT_COUNT), 1);
    assert_eq!(m.content(c, content::PREFIX_START_UTF16), 8);
    assert_eq!(m.content(c, content::START_UTF16), 10);
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
    assert!(Extractor::extract(&src).is_err(), "production extraction must reject a lossy model");
}

#[test]
fn indented_table_body_strong_range_is_exact() {
    let src = "| abc | def |\n| --- | --- |\n| bar |&\n **:**|\n| bar | baz | boo |\n";
    let m = M::of(src); m.clean();
    let r = (0..m.n(header::RUN_COUNT)).find(|&r| m.run(r, run::KIND) == run_kind::STRONG as usize).unwrap();
    assert_eq!(&src[m.run(r, run::START_BYTE)..m.run(r, run::END_BYTE)], "**:**");
    assert_eq!(&src[m.run(r, run::CONTENT_START_BYTE)..m.run(r, run::CONTENT_END_BYTE)], ":");
    assert!(Extractor::extract(src).is_ok());
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

#[test]
fn item_marker_endpoints_are_source_ranges_not_display_columns() {
    for (src, marker) in [
        ("-\tfoo", "-\t"), ("1.\tfoo", "1.\t"),
        ("> -\tfoo", "-\t"), ("- a\n\t- b", "- "),
        ("-\t[x] foo", "-\t"), ("- ## title", "- "),
        ("- a\r\n\t- b", "- "),
    ] {
        let m = M::of(src); m.clean();
        let item = (0..m.n(header::BLOCK_COUNT)).rfind(|&i| m.block(i, block::KIND) == block_kind::ITEM as usize).unwrap();
        let start = m.block(item, block::START_BYTE);
        let end = m.block(item, block::MARKER_END_BYTE);
        assert_eq!(&src[start..end], marker, "for {src:?}");
        assert_eq!(m.block(item, block::MARKER_END_UTF16), src[..end].encode_utf16().count());
    }
}

#[test]
fn a_row_short_of_the_header_columns_has_empty_cells_not_delimiters() {
    for (first, next) in [("", ""), ("> ", "> "), ("- ", "  ")] {
      for ending in ["\n", "\r\n"] {
        for (columns, body, cells) in [
            (2, "| c |", vec![" c ", ""]),
            (2, "| c", vec![" c", ""]),
            (2, "c |", vec!["c ", ""]),
            (2, "| c |   ", vec![" c ", ""]),
            (3, "| c |", vec![" c ", "", ""]),
            (3, "| c | d", vec![" c ", " d", ""]),
            // A row that is not short keeps every derived range.
            (2, "|  |  |", vec!["  ", "  "]),
            (2, "| c ||", vec![" c ", ""]),
            (2, "| c | d |", vec![" c ", " d "]),
            (2, "| c \\| d |", vec![" c \\| d ", ""]),
        ] {
            let head: String = (0..columns).map(|i| format!("| h{i} ")).collect::<String>() + "|";
            let delim: String = (0..columns).map(|_| "| --- ").collect::<String>() + "|";
            let src = format!("{first}{head}{ending}{next}{delim}{ending}{next}{body}{ending}");
            let m = M::of(&src); m.clean();
            let row = (0..m.n(header::BLOCK_COUNT))
                .filter(|&b| m.block(b, block::KIND) == block_kind::TABLE_ROW as usize)
                .next_back().unwrap();
            let body_cells: Vec<usize> = (0..m.n(header::BLOCK_COUNT))
                .filter(|&b| m.block(b, block::KIND) == block_kind::TABLE_CELL as usize
                    && m.block(b, block::FIRST_LINE) >= m.block(row, block::FIRST_LINE))
                .collect();
            let text: Vec<&str> = body_cells.iter()
                .map(|&b| &src[m.block(b, block::START_BYTE)..m.block(b, block::END_BYTE)])
                .collect();
            assert_eq!(text, cells, "for {src:?}");
            // Every cell stays on its own line and inside the row.
            let line_end = src[m.block(row, block::START_BYTE)..].find(['\n', '\r'])
                .map_or(src.len(), |i| m.block(row, block::START_BYTE) + i);
            for &b in &body_cells {
                assert!(m.block(b, block::END_BYTE) <= line_end,
                    "cell crosses its line in {src:?}");
                assert!(m.block(b, block::START_BYTE) <= m.block(b, block::END_BYTE));
            }
        }
      }
    }
}

#[test]
fn a_multiline_tag_or_code_span_ends_on_its_own_line() {
    // comrak places the end of an inline literal that crosses lines with the
    // prefix width of the paragraph line numbered by the lines it crosses,
    // not of the line it ends on. They differ when the literal starts after
    // the paragraph's first line and ends on a line with another prefix: a
    // lazy line has none, and a partial tab leaves virtual spaces.
    for ending in ["\n", "\r\n"] {
        for (first, next, last) in [
            ("> ", "> ", ""), ("- ", "  ", ""), ("> > ", "> > ", ""), ("1. ", "   ", ""),
            ("> 1. ", ">    ", ">\t"), ("> ", "> ", "> "), ("", "", ""),
        ] {
            for (kind, open, close, after) in [(run_kind::HTML_INLINE, "<a", "b>", " q"), (run_kind::CODE, "`a", "b`", " `")] {
                for middle in ["", "c"] {
                    let middle = if middle.is_empty() { String::new() } else { format!("{next}{middle}{ending}") };
                    let src = format!("{first}x{ending}{next}{open}{ending}{middle}{last}{close}{after}");
                    let m = M::of(&src); m.clean();
                    let r = (0..m.n(header::RUN_COUNT)).find(|&r| m.run(r, run::KIND) == kind as usize).unwrap_or_else(|| panic!("no run for {src:?}"));
                    let (s, e) = (src.find(open).unwrap(), src.rfind(close).unwrap() + close.len());
                    assert_eq!((m.run(r, run::START_BYTE), m.run(r, run::END_BYTE)), (s, e), "for {src:?}");
                }
            }
        }
    }
}

#[test]
fn a_blank_line_short_of_an_items_indent_still_belongs_to_its_block() {
    // cmark continues an item that holds a block through a blank line,
    // advancing to its first non-space, even without the item's indentation.
    // Typing a fence in a list item leaves it open over such lines.
    for src in ["- ```\n \n", "- ```\n  x\n \n", "+\t```\n \n", "-\t```\n \n", "1. ```\n \r", "1. - ```\n\t\n", "- a\n\n      code\n \n      more\n"] {
        let m = M::of(src); m.clean();
        for c in 0..m.n(header::CONTENT_COUNT) {
            let (s, e) = (m.content(c, content::START_BYTE), m.content(c, content::END_BYTE));
            assert!(src[s..e].trim().len() == e - s, "content {s}..{e} {:?} keeps a blank line's partial indent in {src:?}", &src[s..e]);
        }
    }
}

#[test]
fn an_empty_container_covers_its_own_empty_lines() {
    // comrak ends an empty footnote definition followed by a blank line inside
    // an item at its label's first byte, short of the definition's own line.
    for src in ["- [^1]:\n\n", "1. [^1]:\n\n", "> - [^1]:\n\n", "- [^1]:\n\n  x\n"] {
        let m = M::of(src); m.clean();
        let def = (0..m.n(header::BLOCK_COUNT)).find(|&b| m.block(b, block::KIND) == block_kind::FOOTNOTE_DEFINITION as usize).unwrap();
        assert_eq!(&src[m.block(def, block::START_BYTE)..m.block(def, block::START_BYTE) + 5], "[^1]:", "for {src:?}");
        assert!(m.block(def, block::END_BYTE) >= src.find(':').unwrap() + 1, "for {src:?}");
    }
}

#[test]
fn a_task_checkbox_after_a_partial_tab_or_on_the_second_line_is_skipped() {
    // comrak reports no list padding for task items. The content column
    // follows cmark's padding rule from where the container's content begins,
    // and the checkbox leads the first paragraph line after its whitespace.
    for src in [">\t- [x]\tb", ">\t1. [ ]", ">\t1. [x]", ">\t1.\t[x]", ">\t1. [ ]\n:", "1.\n   [ ]\\)", "*\n\t [x]", "+\n\t [x]\\!", "1.\n\t1. [x]", "10.\n    [x] ten"] {
        let m = M::of(src); m.clean();
        let checkbox = src.rfind(['[']).unwrap();
        for c in 0..m.n(header::CONTENT_COUNT) {
            let (s, e) = (m.content(c, content::START_BYTE), m.content(c, content::END_BYTE));
            assert!(!(s <= checkbox && checkbox < e), "checkbox inside content {s}..{e} in {src:?}");
        }
    }
}

#[test]
fn nodes_after_a_link_whose_parentheses_span_lines_are_placed_exactly() {
    // comrak does not count a line ending inside a link's parentheses, so
    // every later node of the paragraph is reported a line early. Text,
    // entities, tags and code spans are found from their literals, and
    // emphasis is re-derived around its children.
    for src in [
        "[](\n)\n&41", "[](\n )&m", "[](\n )&x", "[](\n )\t]", "[](\n)\n&mp;", "[](\n )&#1;", "[](\n)x]\n&mp;",
        "[](\n)\n<v>", "[](\n )&*<v>", "[](\n)\n&1;<v>", "[](\n)\t\n*|<v>",
        "[](\n)\n*]*", "_[](\n )_", "_[](\n)\n)_", "[](\n)_n\n;_", "[](\n)_n\n&amp;_", "~[](\n)\n;~", "~[](\n)\n&#1;~",
        "[](\n)``\n` `", "[](\r\n)``\n` `", "[](\n)``\n&mp;",
        "\\&amp; b", "x \\&ouml; y",
    ] {
        let m = M::of(src); m.clean();
    }
}

#[test]
fn a_code_span_of_spaces_around_a_tab_drops_one_space_each_side() {
    // A tab is not a space for the stripping rule, so ` \t ` displays "\t".
    for (src, content) in [("` \t `", "\t"), ("` \t\t `", "\t\t"), ("``` \t ```", "\t"), ("`   `", "   ")] {
        let m = M::of(src); m.clean();
        assert_eq!(m.run_contents(src).iter().find(|(k, _)| *k == run_kind::CODE as usize).map(|(_, t)| *t), Some(content), "for {src:?}");
    }
}

#[test]
fn an_html_block_after_a_partial_tab_ends_on_its_own_line() {
    // comrak's end column counts the tab's virtual spaces and ran into the
    // next line; the block, and a list ending with it, overlapped the next one.
    for src in [">\t<v>\n*", ">\t<v>\nm", ">\t<d>\n.", "+\n\t<v>\nb", "-\n\t<v>\n#", "-\n\t<v>\n-", ">\t<v>\n\n*", "- <v>\n\t\\\n[", "- 1. (\n\t\\\n>"] {
        // M::of checks the structural invariants, sibling overlap included.
        let m = M::of(src); m.clean();
    }
}

#[test]
fn a_task_checkbox_and_a_definition_resolve_in_comraks_order() {
    // comrak strips leading definitions first, then takes a checkbox leading
    // what remains, even on a lazy line; `[x] ` cannot begin a definition, so
    // a definition after the checkbox stays text. A checkbox alone after the
    // definitions leaves no paragraph: its line is the item's, not a gap.
    for (src, text) in [
        ("1. [a]:u\n[ ]", None), ("1. [a]:a\n[x]", None), ("- 1. [a]:a\n[x]", None), ("- [a]: /u\n[x] done", Some("done")), ("- [a]::\n[ ]", None), ("1.\t[a]:`\n[ ]", None),
        ("- [x] [a]:;\n.", Some("[a]:;")), ("- [x] [1]:n\n]", Some("[1]:n")), ("1. [ ]\t[a]:u\n[", Some("[a]:u")),
    ] {
        let m = M::of(src); m.clean();
        let checkbox = src.rfind(['[']).filter(|&i| src[i..].starts_with("[ ]") || src[i..].starts_with("[x]")).or_else(|| src.find("[ ]").or_else(|| src.find("[x]"))).unwrap();
        for c in 0..m.n(header::CONTENT_COUNT) {
            let (s, e) = (m.content(c, content::START_BYTE), m.content(c, content::END_BYTE));
            assert!(!(s <= checkbox && checkbox < e), "checkbox inside content {s}..{e} in {src:?}");
        }
        if let Some(text) = text {
            assert!(m.run_contents(src).iter().any(|(k, t)| *k == run_kind::TEXT as usize && t.starts_with(text)), "no text run starting {text:?} in {src:?}");
        }
    }
}

#[test]
fn escaped_pipes_shift_only_their_own_line_and_run_in_pairs() {
    // comrak unescapes `\|` in cells and in a paragraph it split to make a
    // table header. Two escapes in a row keep both pipes, and in a split
    // paragraph an escape moves only later columns of its own line.
    for src in ["\\|\\|http://.\n:-", "]\n:-\n\\|\\|", "\\|\\|;\\>(\n-|", "&#1;\\|=bé\n|-", "\\|\n&\r\n<\n|-", "\\|\n\\| a\nb\n|-", "\\|\n*[*\n]\n-|", "a \\\\| b\n:-"] {
        let m = M::of(src); m.clean();
    }
}

#[test]
fn a_leading_byte_order_mark_precedes_every_line_zero_derivation() {
    // comrak skips a BOM that begins the first line without counting a
    // column. It is neither content nor a liftable prefix: every first-line
    // content record and prefix starts after it.
    for (body, contents) in [
        ("hello *world*\n", vec!["hello *world*"]),
        ("# Title\n\ntext\n", vec!["Title", "text"]),
        ("- item\n- two\n", vec!["item", "two"]),
        ("> quote\n", vec!["quote"]),
        ("[a]: /u\n\n[a]\n", vec!["[a]"]),
        ("\n# h\n", vec!["h"]),
        ("", vec![]),
    ] {
        let src = format!("\u{feff}{body}");
        let m = M::of(&src); m.clean();
        assert_eq!(m.contents(&src), contents, "for {src:?}");
        for c in 0..m.n(header::CONTENT_COUNT) { assert!(m.content(c, content::PREFIX_START_BYTE) >= 3, "the mark lifted as a prefix in {src:?}"); }
    }
    let src = "\u{feff}```js\ncode\n```";
    let m = M::of(src); m.clean();
    let fence = m.blocks_of(block_kind::CODE_BLOCK)[0];
    assert_eq!(&src[m.block(fence, block::ATTR1)..m.block(fence, block::ATTR2)], "js");
    let src = "\u{feff}```\ncode\n```";
    let m = M::of(src); m.clean();
    let fence = m.blocks_of(block_kind::CODE_BLOCK)[0];
    assert_eq!((m.block(fence, block::ATTR1), m.block(fence, block::ATTR2)), (6, 6), "a bare fence's empty info string sits after its fence");
}

#[test]
fn table_body_rows_start_at_their_own_first_nonspace() {
    // comrak starts a body row at the header's column, as it does the row's
    // cells: under a header after a BOM or indented unlike the header, the
    // row would start inside its first cell or before its own line.
    for src in ["\u{feff}| a |\n|---|\n| b |", "  | a |\n|---|\n| b |\n", "| a |\n|---|\n  | b |\n", "> | a |\n> |---|\n>   | b |\n"] {
        let m = M::of(src); m.clean();
        let rows: Vec<&str> = m.blocks_of(block_kind::TABLE_ROW).into_iter().map(|b| &src[m.block(b, block::START_BYTE)..m.block(b, block::END_BYTE)]).collect();
        assert_eq!(rows, ["| a |", "| b |"], "for {src:?}");
    }
}

#[test]
fn an_entity_piece_displays_exactly_its_decoding() {
    // A reference followed by a cell's escaped pipe displays its decoding,
    // and the pipe's backslash hides on its own. Accepting any display from
    // a reference let `&amp;` show `&|` and left the source `\|` in no run,
    // shown again by a host.
    for (src, cell) in [
        ("| a |\n|---|\n| &amp;\\| |\n", "&|"),
        ("| a |\n|---|\n| &nbsp; \\| x |\n", "\u{a0} | x"),
        ("| a | b |\n|---|---|\n| `a` &amp;\\| bitwise or | z |\n", "a &| bitwise or"),
        ("a &amp;\\| b\n|---|\n", "a &| b"),
    ] {
        let m = M::of(src); m.clean();
        let cells = m.blocks_of(block_kind::TABLE_CELL);
        let shown: Vec<String> = cells.iter().map(|&b| m.shown(src, b)).collect();
        assert!(shown.iter().any(|s| s == cell), "{shown:?} for {src:?}");
    }
    // Ten rows of thirty-two references before escaped pipes, inside every
    // live limit, took seconds while the relocation search tried every window.
    let src = format!("| a |\n|---|\n{}", format!("| {} |\n", "&amp;\\|".repeat(32)).repeat(10));
    let m = M::of(&src); m.clean();
    for b in m.blocks_of(block_kind::TABLE_CELL).into_iter().skip(1) { assert_eq!(m.shown(&src, b), "&|".repeat(32)); }
}

#[test]
fn text_after_a_link_whose_parentheses_span_lines_is_shown_once() {
    // The registered line-early repair relocates text by its literal. A
    // literal ending in `&` must not match the first byte of `&amp;`, and a
    // window too short for the literal must not be explained by an entity
    // that displays the text after it.
    for (src, last) in [
        ("[a](/u \"t\nt\")\nsee &amp; ok\n", "see & ok"),
        ("> See [the guide](/guide \"The\n> guide\") now.\n> Then read it.\n> A &amp; B\n", "A & B"),
        ("a [of FAQ](/u \"to now it\n\")a to of\na &amp;\n", "a &"),
        ("[](\n) \n~~&amp;\\*", "~~&*"),
    ] {
        let m = M::of(src); m.clean();
        let p = m.blocks_of(block_kind::PARAGRAPH)[0];
        assert_eq!(m.shown(src, p).lines().last(), Some(last), "for {src:?}");
    }
}

#[test]
fn a_slice_explains_a_literal_only_piece_by_piece() {
    // Containing `&` or a tab, or sitting in a CRLF paragraph, explained any
    // literal: a drifted run published as a replacement over source it does
    // not hold, crossing the block's end (refusing the document) or showing
    // its text twice. Such a leaf now shows its source.
    for src in [
        "1. a then then [FAQ FAQ](/u \"in and a\n   and the\") it\n   in\n   it and of &amp; FAQ\n",
        "- [](/u \"read\n  read of\") read a\n  read to now\n  in and in &copy; FAQ\n",
        "[](a \"\n\")aaaa\na\nxyzzy &copy;\n",
        "[](a \"\n\")aaaa\na\nabcde\t\n",
        "[](a \"\n\")a\u{672c}a\na\r\naaaaa",
        "[](\n)aaaaa\na\naaa\r\n",
        "[](\na)aaaa\na\naa\r\n-",
    ] {
        assert!(published(src).contains(&"text-mismatch".to_string()), "for {src:?}");
    }
}

#[test]
fn escapes_links_and_footnote_references_keep_their_own_delimiters() {
    // After a line-early drift the child text of `\\` can match the
    // escaping backslash, an empty link can land on `]()`, a reference link
    // can end inside `[][a]`, and a one-character text can match inside
    // `[^1]`. Each leaf shows its source rather than a run that leaves
    // source in no run.
    for (src, rule) in [
        ("[](\n)\n\t\\\\", "escape-delims"),
        ("[](\n)a\na\n[]()", "link-delims"),
        ("[a]:a\n[](\n)\na[][a]", "link-delims"),
        ("[^1]:[](\n)a\na\n\\>1[^1]", "footnote-ref-delims"),
    ] {
        assert!(published(src).contains(&rule.to_string()), "no {rule} for {src:?}");
    }
    // A link's text taken from comrak's child positions before the child's own
    // repair follows the child's runs.
    let src = "![fo [bar](/url\n)](/url2)\n";
    let m = M::of(src); m.clean();
    let image = (0..m.n(header::RUN_COUNT)).find(|&r| m.run(r, run::KIND) == run_kind::IMAGE as usize).unwrap();
    assert_eq!(&src[m.run(image, run::CONTENT_START_BYTE)..m.run(image, run::CONTENT_END_BYTE)], "fo [bar](/url\n)");
}

#[test]
fn a_break_after_a_sibling_ending_inside_a_crlf_does_not_panic() {
    // The break repair sliced from a drifted sibling's end to its line's
    // content end; a sibling ending between the CR and LF made that range
    // reversed and the slice panicked.
    published("[](\naa)a\naa\n[a](\naa)\n\\\r\n\u{e9}");
}

#[test]
fn a_definition_title_may_hold_nul() {
    // comrak's title scanner ends only at 0xFF, which UTF-8 never holds.
    let src = "[a]: /u \"x\0y\"\n\n[a]\n";
    let m = M::of(src); m.clean();
    assert_eq!(m.defs(src), ["[a]: /u \"x\0y\"\n"]);
    let src = "[a]:a\n[a]:a \"\0\"";
    let m = M::of(src); m.clean();
    assert_eq!(m.defs(src).len(), 2);
}

#[test]
fn definitions_a_setext_underline_resolved_stay_out_of_a_paragraph_a_table_splits() {
    // The underline resolves the definitions and becomes paragraph text; the
    // table that later splits the paragraph numbers the split paragraph and
    // its own header from the definitions' stale start line.
    for (src, contents, defs) in [
        ("[r]: /ref\n---\n| a | b |\n|---|---|\n| c | d |\n", vec!["---", "a ", "b ", "c ", "d "], vec!["[r]: /ref\n"]),
        ("[r]: /ref\n===\na|b\n-|-", vec!["===", "a", "b"], vec!["[r]: /ref\n"]),
        ("> [r]: /ref\n> ===\n> x\n> | a |\n> |---|\n", vec!["===", "x", "a "], vec!["[r]: /ref\n"]),
        ("- [r]: /ref\n  ---\n  | a |\n  |---|\n", vec!["---", "a "], vec!["[r]: /ref\n"]),
        ("[a]: /a\n[b]:\n/b\n\"t\"\n---\nmore *x*\n| a \\| b |\n|---|\n", vec!["---", "more *x*", "a \\| b "], vec!["[a]: /a\n", "[b]:\n/b\n\"t\"\n"]),
        ("[r]: /ref\r\n---\r\n| a |\r\n|---|\r\n", vec!["---", "a "], vec!["[r]: /ref\r\n"]),
    ] {
        let m = M::of(src); m.clean();
        assert_eq!(m.contents(src), contents, "for {src:?}");
        assert_eq!(m.defs(src), defs, "for {src:?}");
    }
    // Without the underline the split paragraph keeps the definition as text.
    let src = "[r]: /ref\n| a |\n|---|\n";
    let m = M::of(src); m.clean();
    assert_eq!(m.contents(src), ["[r]: /ref", "a "]);
}

#[test]
fn a_line_of_email_addresses_is_refused_before_it_can_overflow_the_stack() {
    // comrak links the addresses of one text node recursively, about 330
    // bytes of stack each, and a stack overflow aborts the process. Packed as
    // tightly as comrak links them, five bytes each, the editor's widest
    // default line links 819; 1,024 take about 330 KiB, under the room a
    // 512 KiB thread has for 1,582.
    let packed = |n: usize| format!("a@b.c{}\n", "+@d.e".repeat(n - 1));
    let outcome = std::thread::Builder::new().stack_size(512 << 10).spawn(move || {
        [819, 1024, 1025, 16_000].map(|n| Extractor::extract(&packed(n)).is_ok())
    }).unwrap().join().unwrap();
    assert_eq!(outcome, [true, true, false, false]);
    // A line's bytes bound it too: 4,000 `@` signs leave room for 800.
    assert!(Extractor::extract(&"@".repeat(4_000)).is_ok());
    // comrak links addresses in decoded text: a reference to `@` counts.
    assert!(Extractor::extract(&"a&#64;b.c x&commat;y.z ".repeat(513)).is_err());
    let src = packed(1025);
    let (w, devs) = Extractor::extract_with_report(&src);
    check_invariants(&src, &w).unwrap();
    assert_eq!(devs.iter().map(|d| (d.rule, d.leaf)).collect::<Vec<_>>(), [("autolink-depth", None)]);
}
