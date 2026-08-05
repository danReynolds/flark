use comrak::inline_fragment::{
    EMPTY_REFERENCE_SNAPSHOT, InlineFragmentRequest, InlineInputKind, InlineProfile,
    parse_inline_fragment,
};
use std::sync::Mutex;

static SOURCE: Mutex<Option<String>> = Mutex::new(None);

#[unsafe(no_mangle)]
pub extern "C" fn inline_fragment_prepare(bytes: u32, shape: u32) -> u32 {
    let source = generate(bytes as usize, shape);
    let len = source.len() as u32;
    *SOURCE.lock().unwrap() = Some(source);
    len
}

#[unsafe(no_mangle)]
pub extern "C" fn inline_fragment_sample() -> u64 {
    let source = SOURCE.lock().unwrap();
    let logical = source.as_deref().unwrap_or("");
    let request = InlineFragmentRequest {
        logical,
        leaf_id: 1,
        kind: InlineInputKind::Paragraph,
        profile: InlineProfile::Gfm,
        reference_snapshot: &EMPTY_REFERENCE_SNAPSHOT,
        revision: 1,
        expected_revision: 1,
    };
    match parse_inline_fragment(request) {
        Ok(fragment) => {
            let facts = fragment.facts.len() + fragment.projection_facts.len();
            ((facts as u64) << 32) | fragment.output_bytes() as u64
        }
        Err(_) => u64::MAX,
    }
}

fn generate(bytes: usize, shape: u32) -> String {
    let atom = match shape {
        0 => "**bold** *em* `code` ",
        1 => "[link](https://example.com) ![img](x.png) ",
        _ => "~~strike~~ <em>x</em> www.example.com ",
    };
    let mut source = String::with_capacity(bytes);
    while source.len() + atom.len() <= bytes {
        source.push_str(atom);
    }
    source.push_str(&"x".repeat(bytes - source.len()));
    source
}
