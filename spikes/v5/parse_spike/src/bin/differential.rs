//! Runs the render-model extraction over the CommonMark and GFM corpora and
//! reports every deviation the extractor flags, grouped by category.
use flark_parse_spike::model::Extractor;
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Deserialize)]
struct Case { markdown: String, example: u32, section: String }

fn main() {
    let root = std::env::args().nth(1).expect("fixture dir");
    let mut total = 0; let mut with_dev = 0;
    let mut by_cat: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for file in ["common_mark_tests.json", "gfm_tests.json"] {
        let text = std::fs::read_to_string(format!("{root}/{file}")).expect("read");
        let cases: Vec<Case> = serde_json::from_str(&text).expect("json");
        for c in &cases {
            total += 1;
            let (buf, devs) = Extractor::extract_with_report(&c.markdown);
            assert!(buf.len() >= 36);
            if !devs.is_empty() { with_dev += 1; }
            for d in devs {
                let cat = d.split(':').next().unwrap_or("?").to_string();
                by_cat.entry(cat).or_default().push(format!("{file} #{} [{}]: {}", c.example, c.section, d));
            }
        }
    }
    println!("cases: {total}, cases with deviations: {with_dev}");
    for (cat, items) in &by_cat {
        println!("\n== {cat}: {} ==", items.len());
        for it in items.iter().take(6) { println!("  {it}"); }
        if items.len() > 6 { println!("  ... {} more", items.len() - 6); }
    }
}
