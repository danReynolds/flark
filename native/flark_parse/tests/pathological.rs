//! Pathological shapes, the inputs most likely to crash or stall the parse:
//! nesting as deep as the source allows, lines of one delimiter, wide and
//! ragged tables, crowds of definitions, references, entities and
//! addresses, inline nodes comrak misplaces or that cross lines, and line
//! ending mixtures.
//!
//! `pathological_shapes_never_crash` runs every shape through the C ABI on a
//! 512 KiB thread, less than a Dart isolate or wasm32 in V8 gives the parser
//! (REGISTER.md, email autolinking), in a child process: a stack overflow or
//! an abort ends the child, and the test names the shape it was parsing. The
//! build hook ships release libraries; a debug build gets 2 MiB.
//! Each model the ABI publishes must keep the schema invariants.
//!
//! `pathological_shapes_extract_in_linear_time` times every shape at two
//! sizes eight times apart: a path quadratic in the shape takes about 64
//! times as long, a linear one 8. At the larger size each must also stay
//! within forty times a typical document's time per KiB: a path linear in
//! a document's lines but quadratic in each line's width (table cells
//! scanned from their start for every inline) only shows there. Timing is
//! checked in release builds only.
//!
//! FLARK_PATHOLOGICAL_SCALE multiplies the sizes (default 1: 64 KiB to parse,
//! 32 KiB to time; 16 parses a MiB), FLARK_PATHOLOGICAL_SHAPE keeps the
//! shapes whose names contain it, and FLARK_PATHOLOGICAL_REPORT prints every
//! shape's times (run with `--nocapture`).
mod common;
use common::check_invariants;
use flark_parse::model::Extractor;
use flark_parse::{flark_parse, flark_parse_free, PARSE_EXTRACTION_DEVIATION, PARSE_OK};
use std::time::{Duration, Instant};

/// [unit] repeated to about [bytes], at least once.
fn fill(unit: &str, bytes: usize) -> String { unit.repeat((bytes / unit.len()).max(1)) }

/// Lines of [unit] about 4,000 bytes wide, under the editor's 4,096-unit
/// line, to [bytes] in all.
fn lines(unit: &str, bytes: usize) -> String { fill(&(fill(unit, 4_000.min(bytes)) + "\n"), bytes) }

/// A paragraph after a link whose parentheses span lines, which comrak
/// numbers a line early from there on.
fn drifted(unit: &str, bytes: usize) -> String { "[](\n)\n".to_string() + &fill(unit, bytes) }

/// A table of [columns] whose rows are [row].
fn table(columns: usize, row: &str, bytes: usize) -> String { "a|".repeat(columns) + "\n" + &"-|".repeat(columns) + "\n" + &fill(&(row.to_string() + "\n"), bytes) }

/// Text with the numeric references `&#256;` onward, as distinct as they come.
fn references(unit: impl Fn(u32) -> String, bytes: usize) -> String {
    let mut s = String::new();
    let mut i = 0;
    while s.len() < bytes { s.push_str(&unit(256 + i % 60_000)); i += 1; if i % 200 == 0 { s.push('\n'); } }
    s
}

type Make = fn(usize) -> String;

fn shapes() -> Vec<(&'static str, Make)> {
    let mut v: Vec<(&'static str, Make)> = vec![
        // Nesting.
        ("quote markers", |n| ">".repeat(n) + " x"),
        ("quotes", |n| "> ".repeat(n / 2) + "x"),
        ("list items", |n| "- ".repeat(n / 2) + "x"),
        ("ordered items", |n| "1. ".repeat(n / 3) + "x"),
        ("quoted items", |n| "> - ".repeat(n / 4) + "x"),
        ("footnote definitions", |n| "[^a]: ".repeat(n / 6) + "x"),
        ("indented items", |n| { let mut s = String::new(); let mut d = 0; while s.len() < n { s.push_str(&"  ".repeat(d % 1500)); s.push_str("- x\n"); d += 1; } s }),
        ("lazy quotes", |n| ">".repeat(n / 3) + " a\n" + &"b\n".repeat(n / 3)),
        ("emphasis delimiters", |n| "*".repeat(n / 2) + "x" + &"*".repeat(n / 2)),
        ("nested emphasis", |n| "*x ".repeat(n / 6) + &"x* ".repeat(n / 6)),
        ("nested strong", |n| "**x ".repeat(n / 8) + &"x** ".repeat(n / 8)),
        ("nested strikethrough", |n| "~~x ".repeat(n / 8) + &"x~~ ".repeat(n / 8)),
        ("nested images", |n| "![".repeat(n / 6) + "a" + &"](u)".repeat(n / 6)),
        ("nested brackets", |n| "[".repeat(n / 5) + "a" + &"](u)".repeat(n / 5)),
        ("links in emphasis", |n| "*[".repeat(n / 6) + "a" + &"](u)*".repeat(n / 6)),
        // One delimiter, or a short pattern, to the end of every line.
        ("pattern *a", |n| lines("*a", n)), ("pattern a*", |n| lines("a*", n)), ("pattern *_", |n| lines("*_", n)),
        ("pattern ~a", |n| lines("~a", n)), ("pattern [a](", |n| lines("[a](", n)), ("pattern ![](", |n| lines("![](", n)),
        ("pattern [^a]", |n| lines("[^a]", n)), ("pattern a <!--", |n| lines("a <!--", n)), ("pattern a <a b='", |n| lines("a <a b='", n)),
        ("pattern `a", |n| lines("`a", n)), ("pattern \\*", |n| lines("\\*", n)), ("pattern [*", |n| lines("[*", n)),
        ("backtick runs", |n| { let mut s = String::new(); let mut k = 1; while s.len() < n { s.push_str(&"`".repeat(k)); s.push(' '); k = k % 200 + 1; if k == 1 { s.push('\n'); } } s }),
        // Tables.
        ("wide header over short rows", |n| table(2_048, "x", n)),
        ("ragged rows under a quote", |n| { let mut s = "> ".to_string() + &"a|".repeat(1_000) + "\n> " + &"-|".repeat(1_000) + "\n"; while s.len() < n { s.push_str("> x\n"); } s }),
        ("table full rows", |n| table(8, "x|y|z|w|v|u|t|s|", n)),
        ("table extra cells", |n| table(1, &"|x".repeat(1_000), n)),
        ("cells of escapes", |n| table(1, &("|".to_string() + &"\\*".repeat(1_900) + "|"), n)),
        ("cells of escaped pipes and emphasis", |n| table(1, &("|".to_string() + &"\\|*a*".repeat(750) + "|"), n)),
        ("cells of entities and escaped pipes", |n| table(1, &("| ".to_string() + &"&amp;\\|".repeat(500) + " |"), n)),
        ("cells of addresses", |n| table(1, &("|".to_string() + &"a@b.c ".repeat(600) + "|"), n)),
        ("cells of tags", |n| table(1, &("|".to_string() + &"<b>".repeat(1_300) + "|"), n)),
        ("paragraph split by a header", |n| fill(&("\\*".repeat(1_900) + "\n"), n) + "a|b\n-|-\n"),
        ("header candidates", |n| { let mut s = "x\n".to_string(); while s.len() < n { s.push_str("-|-\n-|-|-\n"); } s }),
        // Definitions and references.
        ("definitions", |n| { let mut s = String::new(); let mut i = 0; while s.len() < n { s.push_str(&format!("[ref{i}]: /t/{i} \"title\"\n")); i += 1; } s }),
        ("one label defined again and again", |n| fill("[a]: /u\n", n)),
        ("references", |n| { let mut s: String = (0..200).map(|i| format!("[r{i}]: /u{i}\n")).collect(); s.push('\n'); s + &lines("[r7] [nope] ", n) }),
        ("long labels", |n| { let mut s = String::new(); let mut i = 0; while s.len() < n { s.push_str(&format!("[{}{i}]: /u\n", "x".repeat(900))); i += 1; } s }),
        ("an unclosed title", |n| "[a]: /u \"".to_string() + &fill("x\n", n)),
        ("a label over many lines", |n| "[".to_string() + &fill("a\n", n) + "]: /u\n"),
        ("definitions under task items", |n| fill("- [a]: /u\n  [x] t\n", n)),
        // Entity references.
        ("distinct references", |n| references(|i| format!("&#{i};"), n)),
        ("distinct references in text", |n| references(|i| format!("a&#{i};b "), n)),
        ("references in cells", |n| "|a|b|\n|-|-|\n".to_string() + &references(|i| format!("|&#{i};\\||&#{};|\n", i + 1), n)),
        ("references in task items", |n| references(|i| format!("- &#91;x&#93; &#{i};\n"), n)),
        // Addresses and URLs.
        ("addresses", |n| fill(&("a@b.c ".repeat(600) + "\n"), n)),
        ("packed addresses", |n| fill(&("a@.b".to_string() + &"+@.c".repeat(1_000) + "\n"), n)),
        ("addresses through references", |n| fill(&("a&#64;b.c ".repeat(390) + "\n"), n)),
        ("addresses in a quote", |n| fill(&("> ".to_string() + &"a@b.c ".repeat(600) + "\n"), n)),
        ("addresses in emphasis", |n| fill(&("*a@b.c* ".repeat(400) + "\n"), n)),
        ("URLs", |n| lines("http://a.b/c ", n)),
        // comrak scans a `www.` domain to its end for every `_www.` in it
        // when a line ending, not the input's end, closes it: quadratic in
        // a line's width, the last line spared.
        ("www domains with underscores", |n| lines("www._", n) + "x"),
        ("a URL closing in parentheses", |n| fill(&("http://a.b".to_string() + &")".repeat(3_990) + "\n"), n)),
        ("a long local part", |n| "a".repeat(n) + "@b.c"),
        // Line endings, tabs, a byte order mark.
        ("CRLF prose", |n| fill("a *b* [c](d) &amp; `e`\r\n", n)),
        ("CRLF quotes", |n| fill("> a *b*\r\n", n)),
        ("tabbed items", |n| fill("-\ta\n\t-\tb\n", n)),
        ("tabbed quotes", |n| fill(">\t>\t>\ta\n", n)),
        ("a byte order mark", |n| "\u{feff}".to_string() + &fill("| a |\n|---|\n| b |\n\n", n)),
        ("mixed endings", |n| fill("a\r\n> b\n- c\r\n\t d\n", n)),
        // Inline nodes comrak misplaces, or that cross lines.
        ("misplaced emphasis", |n| drifted("x *a* ", n)),
        ("misplaced references", |n| drifted("x &amp; *a* `c`\n", n)),
        ("misplaced references and pipes", |n| drifted("|&#319;\\||&#575;|\n", n)),
        ("misplaced code spans", |n| drifted("x &`c`\nx &amp; *a* `c`\nx c`\n", n)),
        ("misplaced tags", |n| drifted("<b> ", n)),
        ("misplaced references after a definition", |n| fill(&("[a]: /u\n[](\n)\n".to_string() + &(0..300).map(|i| format!("a&#{};b *x* ", 256 + i)).collect::<String>() + "\n\n"), n)),
        ("code spans across lines", |n| fill("`a\nb` x ", n)),
        ("tags across lines", |n| fill("<a\nb='c'> x ", n)),
        ("link titles across lines", |n| fill("[a](/u \"t\nu\") x ", n)),
        ("quoted code spans across lines", |n| fill("> `a\n> b` x\n", n)),
        ("hard breaks", |n| fill("a\\\n", n)),
        ("footnote references", |n| "[^a]: note\n\n".to_string() + &lines("[^a] [^zz] ", n)),
        // Blocks.
        ("an unclosed fence", |n| "```\n".to_string() + &fill("x\n", n)),
        ("an unclosed fence in a quote", |n| "> ```\n".to_string() + &fill("> x\n", n)),
        ("an unclosed comment", |n| "<!--\n".to_string() + &fill("x\n", n)),
        ("setext headings", |n| fill("a\n=\n", n)),
        ("a long setext heading", |n| fill("a\n", n) + "===\n"),
        ("list items", |n| fill("- a\n", n)),
        ("empty items", |n| fill("-\n", n)),
        ("blank lines in an item", |n| "- a\n".to_string() + &fill("\n", n) + "  b\n"),
        ("lazy lines under items", |n| "- ".repeat(8) + "a\n" + &fill("b\n", n)),
    ];
    if let Ok(only) = std::env::var("FLARK_PATHOLOGICAL_SHAPE") { v.retain(|(name, _)| name.contains(only.as_str())); }
    v
}

fn scale() -> usize { std::env::var("FLARK_PATHOLOGICAL_SCALE").ok().and_then(|v| v.parse().ok()).unwrap_or(1) }

/// The return code of the C ABI for [src], with the published model checked.
fn parse(src: &str) -> i32 {
    let (mut out, mut out_len) = (std::ptr::null_mut(), 0u32);
    let rc = flark_parse(src.as_ptr(), src.len() as u32, &mut out, &mut out_len);
    if rc == PARSE_OK {
        let words = unsafe { std::slice::from_raw_parts(out.cast::<u32>(), out_len as usize / 4) }.to_vec();
        flark_parse_free(out, out_len);
        if let Err(e) = check_invariants(src, &words) { panic!("published model breaks {e}"); }
    }
    rc
}

#[test]
fn pathological_shapes_never_crash() {
    if std::env::var("FLARK_PATHOLOGICAL_CHILD").is_ok() {
        // The child: every shape on a thread with the stack a host gives the
        // release library. Unoptimized frames are about three times larger:
        // comrak's address recursion needs room for 1,024 of them.
        let sizes = [1 << 10, (64 << 10) * scale()];
        let stack = if cfg!(debug_assertions) { 2 << 20 } else { 512 << 10 };
        std::thread::Builder::new().stack_size(stack).spawn(move || {
            for (name, make) in shapes() {
                for bytes in sizes {
                    eprintln!("shape: {name}, {bytes} bytes");
                    let rc = parse(&make(bytes));
                    assert!(rc == PARSE_OK || rc == PARSE_EXTRACTION_DEVIATION, "{name} at {bytes} bytes: return code {rc}");
                }
            }
        }).unwrap().join().unwrap();
        return;
    }
    let out = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["pathological_shapes_never_crash", "--exact", "--nocapture", "--test-threads", "1"])
        .env("FLARK_PATHOLOGICAL_CHILD", "1")
        .output().unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    let last = stderr.lines().filter(|l| l.starts_with("shape: ")).last().unwrap_or("before any shape");
    assert!(out.status.success(), "the child ended with {:?} at {last}:\n{}", out.status, stderr.lines().rev().take(8).collect::<Vec<_>>().join("\n"));
}

/// The best of five extraction times of [src].
fn extract_time(src: &str) -> Duration {
    (0..5).map(|_| { let t = Instant::now(); std::hint::black_box(Extractor::extract(src).is_ok()); t.elapsed() }).min().unwrap()
}

/// A dense typical document: headings, paragraphs with every inline, lists,
/// a quote, fenced code, a table and a definition, over and over.
fn typical(bytes: usize) -> String {
    let mut s = String::new();
    let mut i = 0;
    while s.len() < bytes {
        i += 1;
        s.push_str(&format!("## Section {i}\n\nA paragraph with *emphasis*, **strong**, `code`, a [link](https://example.com/{i}) and ~~struck~~ text that wraps across\nlines of ordinary prose.\n\n- item one with *em*\n- item two with **strong**\n  - nested item\n\n> a quote with `code` inside\n\n```dart\nvoid main() {{ print('hi {i}'); }}\n```\n\n| a | b |\n|---|---|\n| 1 | *2* |\n\n[ref{i}]: https://example.com/ref{i}\n\n"));
    }
    s
}

#[test]
fn pathological_shapes_extract_in_linear_time() {
    if cfg!(debug_assertions) { return; }
    let large = (32 << 10) * scale();
    let small = large / 8;
    let norm = { let src = typical(large); extract_time(&src).as_secs_f64() / src.len() as f64 };
    let mut slow = Vec::new();
    for (name, make) in shapes() {
        let (a, b) = (make(small), make(large));
        let (ta, tb) = (extract_time(&a), extract_time(&b));
        let per_byte = tb.as_secs_f64() / b.len() as f64;
        // Generous bounds: noise must not fail them, a quadratic path or one
        // forty times a typical document's cost must. comrak's own worst
        // here, the `www.` domains, costs about fifteen.
        if std::env::var("FLARK_PATHOLOGICAL_REPORT").is_ok() { eprintln!("{name:40} {ta:>12?} {tb:>12?} ratio {:5.1} per-byte {:5.1}x", tb.as_secs_f64() / ta.as_secs_f64(), per_byte / norm); }
        if tb > ta * 24 + Duration::from_micros(500) || per_byte > norm * 40.0 {
            slow.push(format!("{name}: {ta:?} for {} bytes, {tb:?} for {} ({:.1} times a typical document's time per byte)", a.len(), b.len(), per_byte / norm));
        }
    }
    assert!(slow.is_empty(), "slow shapes:\n{}", slow.join("\n"));
}
