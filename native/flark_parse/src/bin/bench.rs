//! Native-side cost of parse + extraction on a dense synthetic document.
use flark_parse::model::Extractor;
use std::time::Instant;

pub fn gen(target: usize) -> String {
    let mut s = String::new(); let mut i = 0;
    while s.len() < target {
        i += 1;
        s.push_str(&format!("## Section {i}\n\nThis is a paragraph with *emphasis*, **strong**, `code`, a [link](https://example.com/{i}) and some ~~struck~~ text that wraps across several lines of ordinary prose so that the row is realistic.\n\n- item one with *em*\n- item two with **strong**\n  - nested item\n\n> a quote with `code` inside\n\n```dart\nvoid main() {{ print('hi {i}'); }}\n```\n\n| a | b |\n|---|---|\n| 1 | *2* |\n\n[ref{i}]: https://example.com/ref{i}\n\n"));
    }
    s
}

fn main() {
    for &size in &[25_000usize, 64_000, 100_000, 256_000] {
        let src = gen(size);
        let mut times = vec![]; let mut out_len = 0;
        for _ in 0..30 {
            let t = Instant::now();
            let buf = Extractor::extract(&src);
            times.push(t.elapsed().as_secs_f64() * 1000.0);
            out_len = buf.len();
        }
        times.sort_by(|a, b| a.partial_cmp(b).unwrap());
        println!("{:>7} bytes  parse+extract p50 {:6.3} ms  min {:6.3} ms  model {:>7} bytes ({:.2}x source)", src.len(), times[times.len() / 2], times[0], out_len, out_len as f64 / src.len() as f64);
    }
}
