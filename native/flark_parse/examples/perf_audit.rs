//! Diagnostic scaling receipt, not an editor input-to-paint qualification.
//! cargo run --release --example perf_audit
use comrak::{parse_document, Arena};
use flark_parse::model::{options, Extractor};
use serde_json::json;
use std::hint::black_box;
use std::time::Instant;

fn source(shape: &str, bytes: usize) -> String {
    let mut out = String::new();
    let mut i = 0;
    while out.len() < bytes {
        match shape {
            "definitions" => out.push_str(&format!("[ref{i}]: /target/{i} \"title\"\n")),
            "inline" => out.push_str("**bold** *word* `code` [link](/url) "),
            "prose" => out.push_str("Ordinary prose with **bold** and a [link](/url).\n\n"),
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
    for shape in ["definitions", "inline", "prose"] {
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
