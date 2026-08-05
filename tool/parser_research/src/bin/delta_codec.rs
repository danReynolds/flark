//! Cross-runtime golden for the disposable v3 syntax-delta wire format.

const MAGIC: u32 = 0x3344_4c46;
const VERSION: u16 = 1;

#[derive(Debug)]
struct SyntaxDelta<'a> {
    base_revision: u32,
    revision: u32,
    before_hash32: u32,
    after_hash32: u32,
    start_index: u32,
    delete_count: u32,
    inserted: Vec<ParsedBlock<'a>>,
}

#[derive(Debug)]
struct ParsedBlock<'a> {
    stable_id: u64,
    source_utf8_length: u32,
    source_utf16_length: u32,
    hidden_ranges: Vec<LocalRange>,
    replacements: Vec<LocalReplacement<'a>>,
}

#[derive(Debug)]
struct LocalRange {
    start_utf16: u32,
    end_utf16: u32,
}

#[derive(Debug)]
struct LocalReplacement<'a> {
    start_utf16: u32,
    end_utf16: u32,
    text: &'a str,
}

fn encode(delta: &SyntaxDelta<'_>) -> Vec<u8> {
    let mut output = Vec::new();
    u32(&mut output, MAGIC);
    u16(&mut output, VERSION);
    u16(&mut output, 0);
    u32(&mut output, delta.base_revision);
    u32(&mut output, delta.revision);
    u32(&mut output, delta.before_hash32);
    u32(&mut output, delta.after_hash32);
    u32(&mut output, delta.start_index);
    u32(&mut output, delta.delete_count);
    u32(
        &mut output,
        delta
            .inserted
            .len()
            .try_into()
            .expect("insert count fits u32"),
    );
    for block in &delta.inserted {
        u64(&mut output, block.stable_id);
        u32(&mut output, block.source_utf8_length);
        u32(&mut output, block.source_utf16_length);
        u16(
            &mut output,
            block
                .hidden_ranges
                .len()
                .try_into()
                .expect("hidden-range count fits u16"),
        );
        u16(
            &mut output,
            block
                .replacements
                .len()
                .try_into()
                .expect("replacement count fits u16"),
        );
        for range in &block.hidden_ranges {
            u32(&mut output, range.start_utf16);
            u32(&mut output, range.end_utf16);
        }
        for replacement in &block.replacements {
            u32(&mut output, replacement.start_utf16);
            u32(&mut output, replacement.end_utf16);
            u32(
                &mut output,
                replacement
                    .text
                    .len()
                    .try_into()
                    .expect("replacement length fits u32"),
            );
            output.extend_from_slice(replacement.text.as_bytes());
        }
    }
    output
}

fn sample() -> SyntaxDelta<'static> {
    SyntaxDelta {
        base_revision: 7,
        revision: 8,
        before_hash32: 0x1122_3344,
        after_hash32: 0xaabb_ccdd,
        start_index: 9,
        delete_count: 2,
        inserted: vec![ParsedBlock {
            stable_id: 0x001f_ffff_ffff,
            source_utf8_length: 42,
            source_utf16_length: 40,
            hidden_ranges: vec![LocalRange {
                start_utf16: 2,
                end_utf16: 4,
            }],
            replacements: vec![LocalReplacement {
                start_utf16: 8,
                end_utf16: 10,
                text: "é",
            }],
        }],
    }
}

fn u16(output: &mut Vec<u8>, value: u16) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn u32(output: &mut Vec<u8>, value: u32) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn u64(output: &mut Vec<u8>, value: u64) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn main() {
    let encoded = encode(&sample());
    println!("delta_codec bytes={} hex={}", encoded.len(), hex(&encoded));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_matches_the_dart_golden() {
        let encoded = encode(&sample());
        assert_eq!(encoded.len(), 78);
        assert_eq!(
            hex(&encoded),
            "464c443301000000070000000800000044332211ddccbbaa090000000200000001000000ffffffff1f0000002a00000028000000010001000200000004000000080000000a00000002000000c3a9",
        );
    }
}
