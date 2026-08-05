//! Minimal cmark-gfm spec-runner compatible frontend for markdown-rs.

use std::io::{self, Read};

use markdown::{to_html_with_options, Options};

fn main() {
    let mut extensions = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--unsafe" => {}
            "-e" => extensions.push(args.next().expect("-e requires an extension name")),
            other => panic!("unsupported cmark compatibility argument: {other}"),
        }
    }

    let mut markdown = String::new();
    io::stdin()
        .read_to_string(&mut markdown)
        .expect("read markdown from stdin");

    let mut options = if extensions.is_empty() {
        Options::default()
    } else {
        // The cmark extension corpus is run with Flark's complete GFM profile.
        // Individual extension selection is not needed for that comparison.
        Options::gfm()
    };
    options.compile.allow_dangerous_html = true;
    options.compile.allow_dangerous_protocol = true;

    let rendered = to_html_with_options(&markdown, &options).expect("parse markdown");
    print!("{rendered}");
}
