//! Prints an FNV-1a 64-bit hash of the render model for every corpus case,
//! for byte-identity comparison against the wasm transport.
use flark_parse::model::Extractor;
use serde::Deserialize;

#[derive(Deserialize)]
struct Case { markdown: String, example: u32 }

pub fn fnv1a(bytes: &[u8]) -> u64 { let mut h: u64 = 0xcbf29ce484222325; for b in bytes { h ^= *b as u64; h = h.wrapping_mul(0x100000001b3); } h }

fn main() {
    let root = std::env::args().nth(1).expect("fixture dir");
    for file in ["common_mark_tests.json", "gfm_tests.json"] {
        let text = std::fs::read_to_string(format!("{root}/{file}")).expect("read");
        let cases: Vec<Case> = serde_json::from_str(&text).expect("json");
        for c in &cases { let buf = Extractor::extract(&c.markdown); println!("{file}#{} {} {:016x}", c.example, buf.len(), fnv1a(&buf)); }
    }
}
