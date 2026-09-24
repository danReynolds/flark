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
    // a definition after the checkbox stays text.
    for (src, text) in [
        ("1. [a]:u\n[ ]", None), ("- [a]: /u\n[x] done", Some("done")), ("- [a]::\n[ ]", None), ("1.\t[a]:`\n[ ]", None),
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
