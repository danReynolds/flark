//! Fault containment: arbitrary bytes never panic and always yield a model
//! that satisfies the schema invariants. Set FLARK_FUZZ_ITERATIONS to run
//! longer (default 2,000; CI uses the default).
use flark_parse::model::Extractor;

#[path = "conformance.rs"]
mod conformance;

struct XorShift(u64);
impl XorShift { fn next(&mut self) -> u64 { let mut x = self.0; x ^= x << 13; x ^= x >> 7; x ^= x << 17; self.0 = x; x } }

const ALPHABET: &[&str] = &["*", "_", "`", "~", "#", ">", "-", "+", "1.", "[", "]", "(", ")", "<", ">", "!", "\\", "|", ":", "\n", "\n\n", " ", "  ", "\t", "a", "b", "foo", "bar", "http://x.y", "&amp;", "&#x41;", "```", "---", "===", "[x]", "[ ]", "é", "日本", "😀", "\r\n", "[^1]", "[^1]: n", "[a]: /u", "\"t\""];

fn random_doc(rng: &mut XorShift) -> String {
    let n = (rng.next() % 40) as usize + 1;
    let mut s = String::new();
    for _ in 0..n { s.push_str(ALPHABET[(rng.next() % ALPHABET.len() as u64) as usize]); }
    s
}

#[test]
fn random_markdown_never_panics_and_keeps_invariants() {
    let iterations: usize = std::env::var("FLARK_FUZZ_ITERATIONS").ok().and_then(|v| v.parse().ok()).unwrap_or(2000);
    let mut rng = XorShift(0x9E3779B97F4A7C15);
    for i in 0..iterations {
        let doc = random_doc(&mut rng);
        let (buf, _devs) = Extractor::extract_with_report(&doc);
        if let Err(e) = conformance::check_invariants(&doc, &buf) { panic!("iteration {i}: {e} for {:?}", doc); }
    }
}

#[test]
fn corpus_mutations_never_panic() {
    let dir = std::env::var("FLARK_CONFORMANCE_DIR").unwrap_or_else(|_| format!("{}/../../test/fixtures/commonmark/upstream", env!("CARGO_MANIFEST_DIR")));
    let text = std::fs::read_to_string(format!("{dir}/gfm_tests.json")).expect("corpus");
    let cases: Vec<serde_json::Value> = serde_json::from_str(&text).unwrap();
    let mut rng = XorShift(7);
    for (i, c) in cases.iter().enumerate() {
        let md = c["markdown"].as_str().unwrap();
        // delete one random scalar, then insert one random alphabet token
        let chars: Vec<char> = md.chars().collect();
        if chars.is_empty() { continue; }
        let del = (rng.next() % chars.len() as u64) as usize;
        let mut mutated: String = chars.iter().enumerate().filter(|(k, _)| *k != del).map(|(_, ch)| ch).collect();
        let ins = (rng.next() % (mutated.len() as u64 + 1)) as usize;
        let ins = (0..=ins).rev().find(|p| mutated.is_char_boundary(*p)).unwrap_or(0);
        mutated.insert_str(ins, ALPHABET[(rng.next() % ALPHABET.len() as u64) as usize]);
        let (buf, _d) = Extractor::extract_with_report(&mutated);
        if let Err(e) = conformance::check_invariants(&mutated, &buf) { panic!("case {i}: {e} for {:?}", mutated); }
    }
}
