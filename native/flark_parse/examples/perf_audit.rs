//! Diagnostic scaling receipt, not an editor input-to-paint qualification.
//! cargo run --release --example perf_audit
use comrak::{parse_document, Arena};
use flark_parse::model::{options, Extractor};
use serde_json::json;
use std::hint::black_box;
use std::time::Instant;

/// Shapes once super-linear: table cells with entities before escaped pipes
/// (the relocation window search), quotes nested as deep as their lazy
/// continuation is long (per-line container ownership), and one paragraph
/// after a link whose parentheses span lines (container refits).
fn source(shape: &str, bytes: usize) -> String {
    match shape {
        "lazy_quotes" => return ">".repeat(bytes / 3) + " a\n" + &"b\n".repeat(bytes / 3),
        "wrapped_link_paragraph" => return "[](\n)\n".to_string() + &"x *a* ".repeat(bytes / 6),
        _ => {}
    }
    let mut out = String::new();
    if shape == "entity_pipe_cells" { out.push_str("| a |\n|---|\n"); }
    let mut i = 0;
    while out.len() < bytes {
        match shape {
            "definitions" => out.push_str(&format!("[ref{i}]: /target/{i} \"title\"\n")),
            "inline" => out.push_str("**bold** *word* `code` [link](/url) "),
            "prose" => out.push_str("Ordinary prose with **bold** and a [link](/url).\n\n"),
            "entity_pipe_cells" => out.push_str(&format!("| {} |\n", "&amp;\\|".repeat(32))),
            _ => unreachable!(),
        }
        i += 1;
    }
    out
}

fn timing(mut action: impl FnMut()) -> serde_json::Value {
    let mut samples = Vec::new();
    for i in 0..30 {
        let start = Instant::now();
        action();
        if i >= 5 { samples.push(start.elapsed().as_micros() as u64); }
    }
    let mut sorted = samples.clone();
    sorted.sort_unstable();
    json!({"samples_us": samples, "p50_us": sorted[12], "p99_us": sorted[24]})
}

fn main() {
    for shape in ["definitions", "inline", "prose", "entity_pipe_cells", "lazy_quotes", "wrapped_link_paragraph"] {
        for kib in [16, 32, 64, 128, 256] {
            let src = source(shape, kib * 1024);
            let expected = Extractor::extract(&src).expect("valid diagnostic input");
            let fingerprint = expected.iter().flat_map(|x| x.to_le_bytes()).fold(
                0xcbf29ce484222325u64, |hash, byte| (hash ^ byte as u64).wrapping_mul(0x100000001b3));
            let parse = timing(|| {
                let arena = Arena::new();
                black_box(parse_document(&arena, &src, &options()));
            });
            let total = timing(|| {
                black_box(Extractor::extract(&src).expect("valid diagnostic input"));
            });
            println!("{}", json!({"shape": shape, "source_bytes": src.len(),
                "model_words": expected.len(), "model_fnv64": format!("{fingerprint:016x}"),
                "parse": parse, "parse_extract": total}));
        }
    }
}
