use std::fmt::Write;

use crate::{Block, BlockKind, Document, Inline, InlineKind};

pub fn render_html(document: &Document) -> String {
    let mut output = String::new();
    for block in &document.blocks {
        render_block(block, &mut output);
    }
    output
}

fn render_block(block: &Block, output: &mut String) {
    match &block.kind {
        BlockKind::Paragraph => {
            output.push_str("<p>");
            render_inlines(&block.inlines, output);
            output.push_str("</p>\n");
        }
        BlockKind::Heading { level } => {
            write!(output, "<h{level}>").unwrap();
            render_inlines(&block.inlines, output);
            writeln!(output, "</h{level}>").unwrap();
        }
        BlockKind::ThematicBreak => output.push_str("<hr />\n"),
        BlockKind::IndentedCode => {
            output.push_str("<pre><code>");
            escape_html(&block.literal, output);
            output.push_str("</code></pre>\n");
        }
        BlockKind::FencedCode { info } => {
            output.push_str("<pre><code");
            if let Some(language) = info
                .split_ascii_whitespace()
                .next()
                .filter(|s| !s.is_empty())
            {
                output.push_str(" class=\"language-");
                escape_html(language, output);
                output.push('"');
            }
            output.push('>');
            escape_html(&block.literal, output);
            output.push_str("</code></pre>\n");
        }
        BlockKind::BlockQuote => {
            output.push_str("<blockquote>\n");
            for child in &block.children {
                render_block(child, output);
            }
            output.push_str("</blockquote>\n");
        }
    }
}

fn render_inlines(inlines: &[Inline], output: &mut String) {
    for inline in inlines {
        match &inline.kind {
            InlineKind::Text(text) => escape_html(text, output),
            InlineKind::SoftBreak => output.push('\n'),
            InlineKind::HardBreak => output.push_str("<br />\n"),
            InlineKind::Code(code) => {
                output.push_str("<code>");
                escape_html(code, output);
                output.push_str("</code>");
            }
            InlineKind::Emphasis => {
                output.push_str("<em>");
                render_inlines(&inline.children, output);
                output.push_str("</em>");
            }
            InlineKind::Strong => {
                output.push_str("<strong>");
                render_inlines(&inline.children, output);
                output.push_str("</strong>");
            }
        }
    }
}

fn escape_html(input: &str, output: &mut String) {
    for character in input.chars() {
        match character {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '"' => output.push_str("&quot;"),
            _ => output.push(character),
        }
    }
}
