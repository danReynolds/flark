//! Purpose-built incremental parser-kernel spike.
//!
//! This deliberately implements only a representative block-state subset. It
//! validates the storage/checkpoint/convergence shape without claiming
//! CommonMark conformance. Full Comrak remains the semantic oracle.

use std::fmt::Write as _;
use std::time::Instant;

#[derive(Clone, Debug, PartialEq, Eq)]
enum FlowState {
    Normal,
    Fence { marker: u8, length: usize },
    HtmlComment,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BlockKind {
    Blank,
    Heading,
    Paragraph,
    Quote,
    ListItem,
    Definition,
    TableCandidate,
    Fence,
    Html,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Checkpoint {
    end: usize,
    state_after: FlowState,
    kind: BlockKind,
    line_hash: u64,
}

#[derive(Debug)]
struct ApplyReceipt {
    reparsed_bytes: usize,
    reparsed_lines: usize,
    converged: bool,
}

#[derive(Debug)]
struct IncrementalKernel {
    source: Vec<u8>,
    checkpoints: Vec<Checkpoint>,
}

impl IncrementalKernel {
    fn new(source: String) -> Self {
        let source = source.into_bytes();
        let checkpoints = parse_all(&source);
        Self {
            source,
            checkpoints,
        }
    }

    fn apply_same_len(&mut self, start: usize, end: usize, replacement: &[u8]) -> ApplyReceipt {
        assert_eq!(end - start, replacement.len());
        self.source[start..end].copy_from_slice(replacement);

        let old_start_index = self
            .checkpoints
            .partition_point(|checkpoint| checkpoint.end <= start);
        let restart = old_start_index
            .checked_sub(1)
            .map_or(0, |index| self.checkpoints[index].end);
        let mut state = old_start_index
            .checked_sub(1)
            .map_or(FlowState::Normal, |index| {
                self.checkpoints[index].state_after.clone()
            });
        let mut offset = restart;
        let mut old_index = old_start_index;
        let mut replacement_checkpoints = Vec::new();
        let mut converged = false;

        while offset < self.source.len() {
            let line_end = next_line_end(&self.source, offset);
            let checkpoint = parse_line(&self.source[offset..line_end], line_end, state);
            state = checkpoint.state_after.clone();
            let can_converge = line_end >= end
                && self
                    .checkpoints
                    .get(old_index)
                    .is_some_and(|old| *old == checkpoint);
            if can_converge {
                converged = true;
                break;
            }
            replacement_checkpoints.push(checkpoint);
            old_index += 1;
            offset = line_end;
        }

        if !converged {
            old_index = self.checkpoints.len();
        }
        let reparsed_end = replacement_checkpoints
            .last()
            .map_or(restart, |checkpoint| checkpoint.end);
        let reparsed_lines = replacement_checkpoints.len();
        self.checkpoints
            .splice(old_start_index..old_index, replacement_checkpoints);
        ApplyReceipt {
            reparsed_bytes: reparsed_end.saturating_sub(restart),
            reparsed_lines,
            converged,
        }
    }

    fn assert_matches_full(&self) {
        assert_eq!(self.checkpoints, parse_all(&self.source));
    }
}

fn parse_all(source: &[u8]) -> Vec<Checkpoint> {
    let mut checkpoints = Vec::new();
    let mut state = FlowState::Normal;
    let mut offset = 0;
    while offset < source.len() {
        let end = next_line_end(source, offset);
        let checkpoint = parse_line(&source[offset..end], end, state);
        state = checkpoint.state_after.clone();
        checkpoints.push(checkpoint);
        offset = end;
    }
    checkpoints
}

fn parse_line(line: &[u8], end: usize, state: FlowState) -> Checkpoint {
    let content = line.strip_suffix(b"\n").unwrap_or(line);
    let trimmed = trim_up_to_three_spaces(content);
    let (state_after, kind) = match state {
        FlowState::Fence { marker, length } => {
            if closing_fence(trimmed, marker, length) {
                (FlowState::Normal, BlockKind::Fence)
            } else {
                (FlowState::Fence { marker, length }, BlockKind::Fence)
            }
        }
        FlowState::HtmlComment => {
            if contains(trimmed, b"-->") {
                (FlowState::Normal, BlockKind::Html)
            } else {
                (FlowState::HtmlComment, BlockKind::Html)
            }
        }
        FlowState::Normal => classify_normal(trimmed),
    };
    Checkpoint {
        end,
        state_after,
        kind,
        line_hash: hash64(line),
    }
}

fn classify_normal(line: &[u8]) -> (FlowState, BlockKind) {
    if line.is_empty() {
        return (FlowState::Normal, BlockKind::Blank);
    }
    if let Some((marker, length)) = opening_fence(line) {
        return (FlowState::Fence { marker, length }, BlockKind::Fence);
    }
    if line.starts_with(b"<!--") {
        let state = if contains(line, b"-->") {
            FlowState::Normal
        } else {
            FlowState::HtmlComment
        };
        return (state, BlockKind::Html);
    }
    if line[0] == b'#' {
        return (FlowState::Normal, BlockKind::Heading);
    }
    if line[0] == b'>' {
        return (FlowState::Normal, BlockKind::Quote);
    }
    if is_list_marker(line) {
        return (FlowState::Normal, BlockKind::ListItem);
    }
    if is_definition(line) {
        return (FlowState::Normal, BlockKind::Definition);
    }
    if line.contains(&b'|') {
        return (FlowState::Normal, BlockKind::TableCandidate);
    }
    (FlowState::Normal, BlockKind::Paragraph)
}

fn opening_fence(line: &[u8]) -> Option<(u8, usize)> {
    let marker = *line.first()?;
    if marker != b'`' && marker != b'~' {
        return None;
    }
    let length = line.iter().take_while(|byte| **byte == marker).count();
    (length >= 3).then_some((marker, length))
}

fn closing_fence(line: &[u8], marker: u8, minimum: usize) -> bool {
    let length = line.iter().take_while(|byte| **byte == marker).count();
    length >= minimum && line[length..].iter().all(u8::is_ascii_whitespace)
}

fn is_list_marker(line: &[u8]) -> bool {
    matches!(line.first(), Some(b'-' | b'+' | b'*'))
        && line.get(1).is_some_and(u8::is_ascii_whitespace)
        || line
            .iter()
            .position(|byte| *byte == b'.' || *byte == b')')
            .is_some_and(|marker| {
                marker > 0
                    && marker <= 9
                    && line[..marker].iter().all(u8::is_ascii_digit)
                    && line.get(marker + 1).is_some_and(u8::is_ascii_whitespace)
            })
}

fn is_definition(line: &[u8]) -> bool {
    line.first() == Some(&b'[')
        && line
            .windows(2)
            .position(|window| window == b"]:")
            .is_some_and(|end| end > 1)
}

fn trim_up_to_three_spaces(line: &[u8]) -> &[u8] {
    let spaces = line
        .iter()
        .take(3)
        .take_while(|byte| **byte == b' ')
        .count();
    &line[spaces..]
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

fn next_line_end(source: &[u8], offset: usize) -> usize {
    source[offset..]
        .iter()
        .position(|byte| *byte == b'\n')
        .map_or(source.len(), |relative| offset + relative + 1)
}

fn hash64(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

fn ordinary_document(target_bytes: usize) -> (String, Vec<usize>) {
    let mut source = String::with_capacity(target_bytes + 128);
    let mut edit_offsets = Vec::new();
    let mut index = 0;
    while source.len() < target_bytes {
        writeln!(source, "## Section {index}").unwrap();
        let edit_offset = source.len();
        writeln!(source, "alpha {index} has **strong** and [link][shared].\n").unwrap();
        edit_offsets.push(edit_offset);
        index += 1;
    }
    source.push_str("[shared]: https://example.com\n");
    (source, edit_offsets)
}

fn main() {
    let (source, offsets) = ordinary_document(1_000_000);
    let started = Instant::now();
    let mut parser = IncrementalKernel::new(source);
    let initial_micros = started.elapsed().as_micros();
    let mut elapsed = Vec::with_capacity(10_000);
    let mut reparsed = Vec::with_capacity(10_000);
    let mut reparsed_lines = Vec::with_capacity(10_000);
    for (iteration, offset) in offsets.into_iter().take(10_000).enumerate() {
        let replacement = if iteration % 2 == 0 { b"A" } else { b"a" };
        let started = Instant::now();
        let receipt = parser.apply_same_len(offset, offset + 1, replacement);
        elapsed.push(started.elapsed().as_nanos() as usize);
        reparsed.push(receipt.reparsed_bytes);
        reparsed_lines.push(receipt.reparsed_lines);
        assert!(receipt.converged);
        if iteration % 1000 == 0 {
            parser.assert_matches_full();
        }
    }
    parser.assert_matches_full();
    elapsed.sort_unstable();
    reparsed.sort_unstable();
    reparsed_lines.sort_unstable();
    println!(
        "purpose_built_kernel bytes={} checkpoints={} checkpoint_bytes={} initial_us={} edits={} apply_ns_p50={} apply_ns_p95={} apply_ns_p99={} apply_ns_max={} reparsed_p95={} reparsed_max={} lines_p95={}",
        parser.source.len(),
        parser.checkpoints.len(),
        parser.checkpoints.len() * std::mem::size_of::<Checkpoint>(),
        initial_micros,
        elapsed.len(),
        percentile(&elapsed, 50),
        percentile(&elapsed, 95),
        percentile(&elapsed, 99),
        elapsed.last().copied().unwrap_or_default(),
        percentile(&reparsed, 95),
        reparsed.last().copied().unwrap_or_default(),
        percentile(&reparsed_lines, 95),
    );
}

fn percentile(values: &[usize], percentile: usize) -> usize {
    values[(values.len() - 1) * percentile / 100]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn localized_edit_converges_after_the_next_unchanged_line() {
        let source = "paragraph one\nparagraph two\nparagraph three\n".to_string();
        let mut parser = IncrementalKernel::new(source);
        let receipt = parser.apply_same_len(10, 11, b"X");
        assert!(receipt.converged);
        assert_eq!(receipt.reparsed_lines, 1);
        assert_eq!(receipt.reparsed_bytes, 14);
        parser.assert_matches_full();
    }

    #[test]
    fn opening_html_comment_propagates_until_its_real_close() {
        let mut source = String::from("<!-x\n");
        for index in 0..10_000 {
            writeln!(source, "paragraph {index}").unwrap();
        }
        source.push_str("-->\nafter\n");
        let mut parser = IncrementalKernel::new(source);
        let receipt = parser.apply_same_len(0, 4, b"<!--");
        assert!(receipt.converged);
        assert!(receipt.reparsed_bytes > 140_000);
        parser.assert_matches_full();
    }

    #[test]
    fn fence_state_is_part_of_the_checkpoint() {
        let source = "```\ninside\n```\nafter\n".to_string();
        let mut parser = IncrementalKernel::new(source);
        let receipt = parser.apply_same_len(0, 3, b"~~~");
        assert!(!receipt.converged);
        assert_eq!(receipt.reparsed_bytes, parser.source.len());
        parser.assert_matches_full();
    }
}
