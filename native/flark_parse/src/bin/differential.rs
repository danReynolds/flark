//! Runs the extraction over the CommonMark and GFM corpora and prints every
//! deviation grouped by rule. Zero deviations is the M1 exit criterion.
use flark_parse::model::Extractor;
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Deserialize)]
struct Case { markdown: String, example: u32, section: String }

fn main() {
    let root = std::env::args().nth(1).expect("fixture dir");
    let (mut total, mut with_dev) = (0, 0);
    let mut by_rule: BTreeMap<&'static str, Vec<String>> = BTreeMap::new();
    for file in ["common_mark_tests.json", "gfm_tests.json"] {
        let text = std::fs::read_to_string(format!("{root}/{file}")).expect("read");
        let cases: Vec<Case> = serde_json::from_str(&text).expect("json");
        for c in &cases {
            total += 1;
            let (_buf, devs) = Extractor::extract_with_report(&c.markdown);
            if !devs.is_empty() { with_dev += 1; }
            for d in devs { by_rule.entry(d.rule).or_default().push(format!("{file} #{} [{}]: {}", c.example, c.section, d.detail)); }
        }
    }
    println!("cases: {total}, cases with deviations: {with_dev}");
    for (rule, items) in &by_rule {
        println!("\n== {rule}: {} ==", items.len());
        for it in items.iter().take(8) { println!("  {it}"); }
        if items.len() > 8 { println!("  ... {} more", items.len() - 8); }
    }
    if with_dev > 0 { std::process::exit(1); }
}
