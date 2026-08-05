//! Minimal cmark-gfm spec-runner compatible frontend for pulldown-cmark.
//!
//! The cmark spec harness invokes a renderer with `--unsafe` and repeated
//! `-e extension` arguments. Pulldown's own CLI uses different flags, so this
//! adapter lets the authoritative cmark-gfm fixtures exercise the Rust parser
//! without modifying either upstream project.

use std::io::{self, Read};

use pulldown_cmark::{html, Options, Parser};

fn main() {
    let mut options = Options::empty();
    let mut args = std::env::args().skip(1);
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--unsafe" => {}
            "-e" => {
                let extension = args.next().expect("-e requires an extension name");
                enable_extension(&mut options, &extension);
            }
            other => panic!("unsupported cmark compatibility argument: {other}"),
        }
    }

    let mut markdown = String::new();
    io::stdin()
        .read_to_string(&mut markdown)
        .expect("read markdown from stdin");

    let parser = Parser::new_ext(&markdown, options);
    let mut rendered = String::new();
    html::push_html(&mut rendered, parser);
    print!("{rendered}");
}

fn enable_extension(options: &mut Options, extension: &str) {
    match extension {
        "table" => options.insert(Options::ENABLE_TABLES),
        "strikethrough" => options.insert(Options::ENABLE_STRIKETHROUGH),
        "tasklist" => options.insert(Options::ENABLE_TASKLISTS),
        "autolink" | "tagfilter" => options.insert(Options::ENABLE_GFM),
        "footnotes" => options.insert(Options::ENABLE_FOOTNOTES),
        other => panic!("unsupported cmark extension: {other}"),
    }
}
