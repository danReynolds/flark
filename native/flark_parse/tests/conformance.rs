//! M1 exit criterion: every upstream CommonMark and GFM case extracts with
//! zero deviations and a structurally valid render model.
use flark_parse::model::Extractor;
use flark_parse::schema::{self, block, content, definition, header, run};
use serde::Deserialize;

#[derive(Deserialize)]
struct Case { markdown: String, example: u32 }

fn corpus_dir() -> String {
    std::env::var("FLARK_CONFORMANCE_DIR").unwrap_or_else(|_| format!("{}/../../test/fixtures/commonmark/upstream", env!("CARGO_MANIFEST_DIR")))
}

fn words(buf: &[u8]) -> Vec<u32> { buf.chunks_exact(4).map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect() }

/// Structural invariants from the schema, checked on any model.
pub fn check_invariants(src: &str, buf: &[u8]) -> Result<(), String> {
    let w = words(buf);
    if w.len() < schema::HEADER_WORDS { return Err("short header".into()); }
    if w[header::MAGIC] != schema::MAGIC || w[header::VERSION] != schema::VERSION { return Err("bad magic/version".into()); }
    let (nl, nb, nc, nr, nd, ns) = (w[header::LINE_COUNT] as usize, w[header::BLOCK_COUNT] as usize, w[header::CONTENT_COUNT] as usize, w[header::RUN_COUNT] as usize, w[header::DEFINITION_COUNT] as usize, w[header::STRING_BYTES] as usize);
    if w[header::SRC_BYTES] as usize != src.len() { return Err("src_bytes".into()); }
    if w[header::SRC_UTF16] as usize != src.encode_utf16().count() { return Err("src_utf16".into()); }
    let lines_off = schema::HEADER_WORDS; let blocks_off = lines_off + nl * 2; let content_off = blocks_off + nb * block::WORDS; let runs_off = content_off + nc * content::WORDS; let defs_off = runs_off + nr * run::WORDS; let strings_off = defs_off * 4 + 0;
    let _ = strings_off;
    let expected_words = defs_off + nd * definition::WORDS;
    if w.len() < expected_words { return Err(format!("buffer short: {} < {}", w.len(), expected_words)); }
    if buf.len() < expected_words * 4 + ns { return Err("strings short".into()); }
    let utf16_of = |b: usize| src[..b.min(src.len())].encode_utf16().count() as u32;
    let is_boundary = |b: usize| b >= src.len() || src.is_char_boundary(b);
    let blk = |i: usize, f: usize| w[blocks_off + i * block::WORDS + f];
    for i in 0..nb {
        let (s, e) = (blk(i, block::START_BYTE) as usize, blk(i, block::END_BYTE) as usize);
        if s > e || e > src.len() { return Err(format!("block {i} range {s}..{e}")); }
        if !is_boundary(s) || !is_boundary(e) { return Err(format!("block {i} not on scalar boundary")); }
        if blk(i, block::START_UTF16) != utf16_of(s) || blk(i, block::END_UTF16) != utf16_of(e) { return Err(format!("block {i} utf16")); }
        let p = blk(i, block::PARENT);
        if i == 0 { if p != u32::MAX { return Err("document parent".into()); } } else if p as usize >= i { return Err(format!("block {i} parent {p}")); }
        let (co, cn) = (blk(i, block::CONTENT_OFFSET) as usize, blk(i, block::CONTENT_COUNT) as usize);
        if co + cn > nc { return Err(format!("block {i} content range")); }
        let mut last_line = None;
        for c in co..co + cn {
            let cw = |f: usize| w[content_off + c * content::WORDS + f];
            let (cs, ce, line) = (cw(content::START_BYTE) as usize, cw(content::END_BYTE) as usize, cw(content::LINE) as usize);
            if cs > ce || cs < s || ce > e + 1 { return Err(format!("block {i} content {c} {cs}..{ce} outside {s}..{e}")); }
            if !is_boundary(cs) || !is_boundary(ce) { return Err(format!("content {c} not on boundary")); }
            if cw(content::START_UTF16) != utf16_of(cs) || cw(content::END_UTF16) != utf16_of(ce) { return Err(format!("content {c} utf16")); }
            if let Some(l) = last_line { if line <= l { return Err(format!("block {i} content lines not increasing")); } }
            last_line = Some(line);
        }
    }
    let mut prev_block = 0u32;
    for i in 0..nr {
        let rw = |f: usize| w[runs_off + i * run::WORDS + f];
        let (s, e, cs, ce) = (rw(run::START_BYTE) as usize, rw(run::END_BYTE) as usize, rw(run::CONTENT_START_BYTE) as usize, rw(run::CONTENT_END_BYTE) as usize);
        if !(s <= cs && cs <= ce && ce <= e && e <= src.len()) { return Err(format!("run {i} order {s} {cs} {ce} {e}")); }
        if !is_boundary(s) || !is_boundary(e) || !is_boundary(cs) || !is_boundary(ce) { return Err(format!("run {i} not on boundary")); }
        if rw(run::START_UTF16) != utf16_of(s) || rw(run::END_UTF16) != utf16_of(e) || rw(run::CONTENT_START_UTF16) != utf16_of(cs) || rw(run::CONTENT_END_UTF16) != utf16_of(ce) { return Err(format!("run {i} utf16")); }
        let b = rw(run::BLOCK); if b as usize >= nb { return Err(format!("run {i} block {b}")); }
        if b < prev_block { return Err(format!("run {i} block order")); } prev_block = b;
        let p = rw(run::PARENT); if p != u32::MAX { if p as usize >= i { return Err(format!("run {i} parent {p}")); } if w[runs_off + p as usize * run::WORDS + run::BLOCK] != b { return Err(format!("run {i} parent in other block")); } }
        let (bs, be) = (blk(b as usize, block::START_BYTE) as usize, blk(b as usize, block::END_BYTE) as usize);
        if s < bs || e > be + 1 { return Err(format!("run {i} {s}..{e} outside block {b} {bs}..{be}")); }
    }
    for i in 0..nd {
        let dw = |f: usize| w[defs_off + i * definition::WORDS + f];
        let (s, e) = (dw(definition::START_BYTE) as usize, dw(definition::END_BYTE) as usize);
        if s > e || e > src.len() { return Err(format!("definition {i} range")); }
        for c in 0..nc { let cw = |f: usize| w[content_off + c * content::WORDS + f]; let (cs, ce) = (cw(content::START_BYTE) as usize, cw(content::END_BYTE) as usize); if cs < e && ce > s && cs < ce { return Err(format!("definition {i} {s}..{e} overlaps content {c} {cs}..{ce}")); } }
    }
    Ok(())
}

#[test]
fn corpora_extract_with_zero_deviations_and_valid_models() {
    let dir = corpus_dir();
    let mut total = 0; let mut failures = Vec::new();
    for file in ["common_mark_tests.json", "gfm_tests.json"] {
        let text = std::fs::read_to_string(format!("{dir}/{file}")).expect("corpus present");
        let cases: Vec<Case> = serde_json::from_str(&text).unwrap();
        for c in &cases {
            total += 1;
            let (buf, devs) = Extractor::extract_with_report(&c.markdown);
            for d in &devs { failures.push(format!("{file} #{}: {} {}", c.example, d.rule, d.detail)); }
            if let Err(e) = check_invariants(&c.markdown, &buf) { failures.push(format!("{file} #{}: invariant {e}", c.example)); }
        }
    }
    assert_eq!(total, 1322, "corpus size");
    assert!(failures.is_empty(), "{} failures:\n{}", failures.len(), failures.join("\n"));
}
