use std::time::Instant;

use flark_owned_parser_trial::{parse, render_html};

fn main() {
    let depth = std::env::args()
        .nth(1)
        .map_or(5_000, |value| value.parse().unwrap());
    let input = format!("{}{}", "*a **a ".repeat(depth), " a** a*".repeat(depth));
    let parse_started = Instant::now();
    let document = parse(&input);
    let parse_micros = parse_started.elapsed().as_micros();
    let render_started = Instant::now();
    let html = render_html(&document);
    println!(
        "owned_nested depth={depth} bytes={} html_bytes={} parse_us={parse_micros} render_us={}",
        input.len(),
        html.len(),
        render_started.elapsed().as_micros()
    );
}
