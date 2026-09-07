//! Bounded language inference using the same pinned parsers and query data.
//! A parser accepting identifiers/prose is not evidence of a code language.
use tree_sitter::{QueryCursor, StreamingIterator};

pub fn detect(source: &str) -> Result<u32, String> {
    if source.encode_utf16().count() > 128 {
        return Err("detection sample limit".into());
    }
    if source.trim().is_empty() {
        return Ok(0);
    }
    let mut best = (0_u64, 0_u32);
    for id in 1..=crate::languages::COUNT {
        let grammar = crate::languages::get(id)?;
        let prepared = crate::syntax::parse_fresh(source, &grammar)?;
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(
            &grammar.detection,
            prepared.tree.root_node(),
            prepared.source.as_bytes(),
        );
        let mut evidence = std::collections::BTreeMap::new();
        while let Some(m) = matches.next() {
            for c in m.captures() {
                if !c.node.is_missing()
                    && c.node.start_byte() >= prepared.offset
                    && c.node.end_byte() <= prepared.offset + source.len()
                {
                    let weight = match grammar.detection.capture_names()[c.index as usize] {
                        "signal.strong" => 8,
                        "signal.weak" => 1,
                        _ => 2,
                    };
                    evidence.insert((c.node.start_byte(), c.node.end_byte()), weight);
                }
            }
        }
        if evidence.is_empty() {
            continue;
        }
        let errors = crate::syntax::error_weight(&prepared.tree, prepared.offset, source.len());
        if errors.0 > 0
            && evidence.first_key_value().is_some_and(|((start, _), _)| {
                !prepared.source[prepared.offset..*start].trim().is_empty()
            })
        {
            continue;
        }
        // Missing closing tokens are routine during typing. Actual ERROR
        // coverage reduces confidence; a query signal is always required.
        let score = evidence.values().sum::<u64>().min(32) * 1000 * (source.len() + 1) as u64
            / (source.len() + 1 + errors.0 * 4 + errors.1) as u64;
        if score > best.0 {
            best = (score, id);
        }
    }
    Ok(best.1)
}
