//! M1 exit criteria over the upstream CommonMark 0.31.2 and GFM corpora.
//!
//! Two separate claims, kept separate:
//! 1. `spec_html`: comrak's rendering equals the fixture HTML for every case
//!    except the registered deviations (spec conformance of the parser).
//! 2. `extraction`: the render model extracts with zero deviations against
//!    comrak's own output and satisfies the schema invariants (fidelity of
//!    the extraction to the parser).
mod common;
use common::{check_invariants, corpus, FILES};
use flark_parse::model::Extractor;

fn spec_options(extensions: &[String]) -> comrak::Options<'static> {
    let mut o = comrak::Options::default();
    o.render.r#unsafe = true;
    for e in extensions {
        match e.as_str() {
            "table" => o.extension.table = true,
            "strikethrough" => o.extension.strikethrough = true,
            "autolink" => o.extension.autolink = true,
            "tagfilter" => o.extension.tagfilter = true,
            "tasklist" => o.extension.tasklist = true,
            _ => {}
        }
    }
    o
}

/// The registered, reviewed deviations from the spec HTML (example numbers).
fn registered_deviations(file: &str) -> Vec<u32> {
    let path = format!("{}/../deviation_register.json", common::corpus_dir());
    let text = std::fs::read_to_string(path).expect("deviation register present");
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    let key = if file.starts_with("common") { "core" } else { "gfm" };
    v[key].as_array().map(|a| a.iter().filter_map(|e| e["example"].as_u64().map(|n| n as u32)).collect()).unwrap_or_default()
}

#[test]
fn spec_html_matches_the_fixtures_except_registered_deviations() {
    let mut total = 0; let mut failures = Vec::new();
    for file in FILES {
        let registered = registered_deviations(file);
        for c in corpus(file) {
            total += 1;
            let html = comrak::markdown_to_html(&c.markdown, &spec_options(&c.extensions));
            if html != c.html && !registered.contains(&c.example) {
                failures.push(format!("{file} #{} [{}]\n  expected {:?}\n  comrak   {:?}", c.example, c.section, c.html, html));
            }
        }
    }
    assert_eq!(total, 1322);
    assert!(failures.is_empty(), "{} unregistered spec deviations:\n{}", failures.len(), failures.join("\n"));
}

#[test]
fn extraction_has_zero_deviations_and_valid_models() {
    let mut total = 0; let mut failures = Vec::new();
    for file in FILES {
        for c in corpus(file) {
            total += 1;
            let (w, devs) = Extractor::extract_with_report(&c.markdown);
            for d in &devs { failures.push(format!("{file} #{}: {} {}", c.example, d.rule, d.detail)); }
            if let Err(e) = check_invariants(&c.markdown, &w) { failures.push(format!("{file} #{}: invariant {e}", c.example)); }
        }
    }
    assert_eq!(total, 1322, "corpus size");
    assert!(failures.is_empty(), "{} failures:\n{}", failures.len(), failures.join("\n"));
}
