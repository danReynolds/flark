//! Fault containment: arbitrary input never panics and always yields a model
//! that satisfies the schema invariants and extracts without deviations.
//! Set FLARK_FUZZ_ITERATIONS to run longer (default 2,000).
//!
//! Bare CR line endings are outside the fidelity contract: comrak's inline
//! line counter does not advance across them, so the kernel normalizes them
//! to LF on load (see REGISTER.md). CRLF is in the alphabet; bare CR is not.
mod common;
use common::{check_invariants, corpus};
use flark_parse::model::Extractor;

struct XorShift(u64);
impl XorShift { fn next(&mut self) -> u64 { let mut x = self.0; x ^= x << 13; x ^= x >> 7; x ^= x << 17; self.0 = x; x } }

const ALPHABET: &[&str] = &["*", "_", "`", "~", "#", ">", "-", "+", "1.", "[", "]", "(", ")", "<", "!", "\\", "|", ":", "\n", "\n\n", "\r\n", " ", "  ", "\t", "a", "b", "foo", "bar", "http://x.y", "&amp;", "&#x41;", "```", "---", "===", "[x]", "[ ]", "é", "日本", "😀", "[^1]", "[^1]: n", "[a]: /u", "\"t\"", "> ", "- ", "1. ", "\\|", "<div>"];

fn random_doc(rng: &mut XorShift) -> String {
    let n = (rng.next() % 40) as usize + 1;
    let mut s = String::new();
    for _ in 0..n { s.push_str(ALPHABET[(rng.next() % ALPHABET.len() as u64) as usize]); }
    s
}

fn check(doc: &str, label: &str) {
    let (w, devs) = Extractor::extract_with_report(doc);
    if let Err(e) = check_invariants(doc, &w) { panic!("{label}: invariant {e} for {:?}", doc); }
    if let Some(d) = devs.first() { panic!("{label}: deviation {} {} for {:?}", d.rule, d.detail, doc); }
}

#[test]
fn random_markdown_never_panics_and_keeps_invariants() {
    let iterations: usize = std::env::var("FLARK_FUZZ_ITERATIONS").ok().and_then(|v| v.parse().ok()).unwrap_or(2000);
    let mut rng = XorShift(0x9E3779B97F4A7C15);
    for i in 0..iterations { let doc = random_doc(&mut rng); check(&doc, &format!("iteration {i}")); }
}

#[test]
fn corpus_mutations_never_panic() {
    let mut rng = XorShift(7);
    for (i, c) in corpus("gfm_tests.json").iter().enumerate() {
        let chars: Vec<char> = c.markdown.chars().collect();
        if chars.is_empty() { continue; }
        let del = (rng.next() % chars.len() as u64) as usize;
        let mut mutated: String = chars.iter().enumerate().filter(|(k, _)| *k != del).map(|(_, ch)| ch).collect();
        let ins = (rng.next() % (mutated.len() as u64 + 1)) as usize;
        let ins = (0..=ins).rev().find(|p| mutated.is_char_boundary(*p)).unwrap_or(0);
        mutated.insert_str(ins, ALPHABET[(rng.next() % ALPHABET.len() as u64) as usize]);
        let (w, _) = Extractor::extract_with_report(&mutated);
        if let Err(e) = check_invariants(&mutated, &w) { panic!("case {i}: {e} for {:?}", mutated); }
    }
}

#[test]
fn deep_nesting_does_not_overflow_the_stack() {
    let quotes = "> ".repeat(20_000) + "x";
    check(&quotes, "20k quotes");
    let lists: String = (0..5_000).map(|d| format!("{}- x\n", "  ".repeat(d))).collect();
    let (w, _) = Extractor::extract_with_report(&lists);
    check_invariants(&lists, &w).unwrap();
    let emph = "*".repeat(5_000) + "x" + &"*".repeat(5_000);
    let (w, _) = Extractor::extract_with_report(&emph);
    check_invariants(&emph, &w).unwrap();
}
