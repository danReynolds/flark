//! Resumable cleaning for parser-authenticated `CommonMark` reference values.
//!
//! The block parser owns recognition and supplies exact destination/title
//! ranges. This crate performs only the donor-selected value transform. It
//! retains at most one HTML-entity candidate and one tiny output chunk; a
//! caller can therefore stream arbitrarily large values into persistent
//! storage without constructing a `String`.

use std::fmt;
use std::ops::Range;

#[allow(clippy::unreadable_literal)] // Generated PHF keys are opaque u64 values.
mod generated {
    include!(concat!(env!("OUT_DIR"), "/entity_map.rs"));
}

/// Comrak inspects at most 32 bytes after `&` while recognizing one entity.
pub const MAX_ENTITY_CANDIDATE_BYTES: usize = 32;
/// One named entity emits no more than this pinned-table maximum.
pub const MAX_NAMED_ENTITY_OUTPUT_BYTES: usize = generated::MAX_NAMED_ENTITY_OUTPUT_BYTES;
/// Complete source spelling (`&name;`) of the longest pinned named entity.
pub const MAX_NAMED_ENTITY_SOURCE_BYTES: usize = generated::MAX_NAMED_ENTITY_SOURCE_BYTES;
/// Tight worst-case output/source ratio across the pinned named-entity table.
pub const MAX_ENTITY_EXPANSION_NUMERATOR: usize = generated::MAX_ENTITY_EXPANSION_NUMERATOR;
pub const MAX_ENTITY_EXPANSION_DENOMINATOR: usize = generated::MAX_ENTITY_EXPANSION_DENOMINATOR;
/// One decoded entity plus a pending literal backslash fits this fixed output.
pub const MAX_CLEAN_OUTPUT_CHUNK_BYTES: usize = 16;

const _: () = assert!(MAX_NAMED_ENTITY_SOURCE_BYTES <= MAX_ENTITY_CANDIDATE_BYTES + 1);
const _: () = assert!(MAX_NAMED_ENTITY_OUTPUT_BYTES <= MAX_CLEAN_OUTPUT_CHUNK_BYTES);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ReferenceValueCleanerReceipt {
    pub polls: u64,
    pub input_bytes: u64,
    pub replayed_bytes: u64,
    pub output_bytes: u64,
    pub entities_decoded: u64,
    pub invalid_entity_fallbacks: u64,
    pub backslashes_removed: u64,
    pub maximum_entity_candidate_bytes: usize,
    pub maximum_output_chunk_bytes: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReferenceValueCleanerStatus {
    /// One bounded transition completed, but internal replay or finalization
    /// work remains before another source byte may be offered.
    Progress,
    NeedInput,
    OutputReady,
    Complete,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReferenceValueCleanerError {
    InputAlreadyPending,
    OutputNotConsumed,
    InputAlreadyFinished,
    OutputNotReady,
    CounterOverflow,
    InternalOutputOverflow,
}

impl fmt::Display for ReferenceValueCleanerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InputAlreadyPending => "one cleaner input byte is already pending",
            Self::OutputNotConsumed => "cleaner output must be consumed before input advances",
            Self::InputAlreadyFinished => "cleaner input is already finished",
            Self::OutputNotReady => "cleaner has no output chunk",
            Self::CounterOverflow => "reference-value cleaner counter overflow",
            Self::InternalOutputOverflow => "reference-value cleaner output bound was exceeded",
        })
    }
}

impl std::error::Error for ReferenceValueCleanerError {}

/// Non-cloneable bounded output returned by one cleaner transition.
#[must_use = "cleaned bytes must be appended to the persistent value sink"]
pub struct CleanReferenceValueChunk {
    bytes: [u8; MAX_CLEAN_OUTPUT_CHUNK_BYTES],
    len: usize,
}

impl CleanReferenceValueChunk {
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes[..self.len]
    }
}

/// Incremental equivalent of Comrak's HTML-entity decode followed by its
/// backslash-unescape pass.
///
/// Destination trimming and title-delimiter removal are separate bounded
/// range-selection steps below. This transducer consumes only the selected
/// body and never needs to rewind it.
#[derive(Debug)]
pub struct ReferenceValueBodyCleaner {
    pending_input: Option<u8>,
    phase: InputPhase,
    entity_state: EntityState,
    entity: [u8; MAX_ENTITY_CANDIDATE_BYTES],
    replay: [u8; MAX_ENTITY_CANDIDATE_BYTES],
    replay_cursor: usize,
    replay_len: usize,
    pending_backslash: bool,
    output: [u8; MAX_CLEAN_OUTPUT_CHUNK_BYTES],
    output_len: usize,
    receipt: ReferenceValueCleanerReceipt,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InputPhase {
    Accepting,
    Finished,
    Complete,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EntityState {
    Idle,
    Candidate { len: usize },
}

impl ReferenceValueBodyCleaner {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            pending_input: None,
            phase: InputPhase::Accepting,
            entity_state: EntityState::Idle,
            entity: [0; MAX_ENTITY_CANDIDATE_BYTES],
            replay: [0; MAX_ENTITY_CANDIDATE_BYTES],
            replay_cursor: 0,
            replay_len: 0,
            pending_backslash: false,
            output: [0; MAX_CLEAN_OUTPUT_CHUNK_BYTES],
            output_len: 0,
            receipt: ReferenceValueCleanerReceipt {
                polls: 0,
                input_bytes: 0,
                replayed_bytes: 0,
                output_bytes: 0,
                entities_decoded: 0,
                invalid_entity_fallbacks: 0,
                backslashes_removed: 0,
                maximum_entity_candidate_bytes: 0,
                maximum_output_chunk_bytes: 0,
            },
        }
    }

    /// Offers the next source-ordered byte.
    ///
    /// # Errors
    ///
    /// Returns an error if another byte or output is pending, or input ended.
    pub fn offer_byte(&mut self, byte: u8) -> Result<(), ReferenceValueCleanerError> {
        if self.phase != InputPhase::Accepting {
            return Err(ReferenceValueCleanerError::InputAlreadyFinished);
        }
        if self.output_len != 0 {
            return Err(ReferenceValueCleanerError::OutputNotConsumed);
        }
        if self.pending_input.is_some() {
            return Err(ReferenceValueCleanerError::InputAlreadyPending);
        }
        let input_bytes = self
            .receipt
            .input_bytes
            .checked_add(1)
            .ok_or(ReferenceValueCleanerError::CounterOverflow)?;
        self.pending_input = Some(byte);
        self.receipt.input_bytes = input_bytes;
        Ok(())
    }

    /// Marks the parser-authenticated body range complete.
    ///
    /// # Errors
    ///
    /// Returns an error when input already ended or a byte/output is pending.
    pub fn finish_input(&mut self) -> Result<(), ReferenceValueCleanerError> {
        if self.phase != InputPhase::Accepting {
            return Err(ReferenceValueCleanerError::InputAlreadyFinished);
        }
        if self.pending_input.is_some() {
            return Err(ReferenceValueCleanerError::InputAlreadyPending);
        }
        if self.output_len != 0 {
            return Err(ReferenceValueCleanerError::OutputNotConsumed);
        }
        self.phase = InputPhase::Finished;
        Ok(())
    }

    /// Advances at most one input, replay, entity-fallback, or finish step.
    ///
    /// # Errors
    ///
    /// Returns an error only on counter or fixed-output-envelope exhaustion.
    pub fn poll(&mut self) -> Result<ReferenceValueCleanerStatus, ReferenceValueCleanerError> {
        self.receipt.polls = self
            .receipt
            .polls
            .checked_add(1)
            .ok_or(ReferenceValueCleanerError::CounterOverflow)?;
        if self.output_len != 0 {
            return Ok(ReferenceValueCleanerStatus::OutputReady);
        }
        if self.phase == InputPhase::Complete {
            return Ok(ReferenceValueCleanerStatus::Complete);
        }

        if self.replay_cursor < self.replay_len {
            let byte = self.replay[self.replay_cursor];
            self.replay_cursor += 1;
            self.receipt.replayed_bytes = self
                .receipt
                .replayed_bytes
                .checked_add(1)
                .ok_or(ReferenceValueCleanerError::CounterOverflow)?;
            if self.replay_cursor == self.replay_len {
                self.replay_cursor = 0;
                self.replay_len = 0;
            }
            self.process_entity_input(byte)?;
            return Ok(self.status_after_transition());
        }

        if let Some(byte) = self.pending_input.take() {
            self.process_entity_input(byte)?;
            return Ok(self.status_after_transition());
        }

        if self.phase == InputPhase::Accepting {
            return Ok(ReferenceValueCleanerStatus::NeedInput);
        }
        if matches!(self.entity_state, EntityState::Candidate { .. }) {
            self.fallback_entity()?;
            return Ok(self.status_after_transition());
        }
        if self.pending_backslash {
            self.pending_backslash = false;
            self.emit(b'\\')?;
            return Ok(ReferenceValueCleanerStatus::OutputReady);
        }
        self.phase = InputPhase::Complete;
        Ok(ReferenceValueCleanerStatus::Complete)
    }

    /// Takes the one bounded clean-output chunk.
    ///
    /// # Errors
    ///
    /// Returns an error when no output is ready.
    pub fn take_output(&mut self) -> Result<CleanReferenceValueChunk, ReferenceValueCleanerError> {
        if self.output_len == 0 {
            return Err(ReferenceValueCleanerError::OutputNotReady);
        }
        let mut bytes = [0; MAX_CLEAN_OUTPUT_CHUNK_BYTES];
        bytes[..self.output_len].copy_from_slice(&self.output[..self.output_len]);
        let len = self.output_len;
        self.output_len = 0;
        Ok(CleanReferenceValueChunk { bytes, len })
    }

    #[must_use]
    pub const fn receipt(&self) -> ReferenceValueCleanerReceipt {
        self.receipt
    }

    fn status_after_transition(&self) -> ReferenceValueCleanerStatus {
        if self.output_len != 0 {
            return ReferenceValueCleanerStatus::OutputReady;
        }
        if self.phase == InputPhase::Accepting
            && self.pending_input.is_none()
            && self.replay_cursor == self.replay_len
        {
            return ReferenceValueCleanerStatus::NeedInput;
        }
        ReferenceValueCleanerStatus::Progress
    }

    fn process_entity_input(&mut self, byte: u8) -> Result<(), ReferenceValueCleanerError> {
        let EntityState::Candidate { mut len } = self.entity_state else {
            if byte == b'&' {
                self.entity_state = EntityState::Candidate { len: 0 };
                return Ok(());
            }
            return self.feed_unescape(byte);
        };

        if len == MAX_ENTITY_CANDIDATE_BYTES {
            return Err(ReferenceValueCleanerError::InternalOutputOverflow);
        }
        self.entity[len] = byte;
        len += 1;
        self.entity_state = EntityState::Candidate { len };
        self.receipt.maximum_entity_candidate_bytes =
            self.receipt.maximum_entity_candidate_bytes.max(len);

        if byte == b';' {
            if let Some(decoded) = decode_entity(&self.entity[..len]) {
                self.entity_state = EntityState::Idle;
                self.receipt.entities_decoded = self
                    .receipt
                    .entities_decoded
                    .checked_add(1)
                    .ok_or(ReferenceValueCleanerError::CounterOverflow)?;
                match decoded {
                    DecodedEntity::Static(text) => {
                        for byte in text.bytes() {
                            self.feed_unescape(byte)?;
                        }
                    }
                    DecodedEntity::Scalar(value) => {
                        let mut encoded = [0; 4];
                        for byte in value.encode_utf8(&mut encoded).bytes() {
                            self.feed_unescape(byte)?;
                        }
                    }
                }
                return Ok(());
            }
            return self.fallback_entity();
        }

        if len == MAX_ENTITY_CANDIDATE_BYTES {
            return self.fallback_entity();
        }
        Ok(())
    }

    fn fallback_entity(&mut self) -> Result<(), ReferenceValueCleanerError> {
        let EntityState::Candidate { len } = self.entity_state else {
            unreachable!("entity fallback requires a candidate")
        };
        debug_assert_eq!(self.replay_len, 0);
        self.replay[..len].copy_from_slice(&self.entity[..len]);
        self.replay_cursor = 0;
        self.replay_len = len;
        self.entity_state = EntityState::Idle;
        self.receipt.invalid_entity_fallbacks = self
            .receipt
            .invalid_entity_fallbacks
            .checked_add(1)
            .ok_or(ReferenceValueCleanerError::CounterOverflow)?;
        self.feed_unescape(b'&')
    }

    fn feed_unescape(&mut self, byte: u8) -> Result<(), ReferenceValueCleanerError> {
        if self.pending_backslash {
            self.pending_backslash = false;
            if is_ascii_punctuation(byte) {
                self.receipt.backslashes_removed = self
                    .receipt
                    .backslashes_removed
                    .checked_add(1)
                    .ok_or(ReferenceValueCleanerError::CounterOverflow)?;
                return self.emit(byte);
            }
            self.emit(b'\\')?;
        }
        if byte == b'\\' {
            self.pending_backslash = true;
            Ok(())
        } else {
            self.emit(byte)
        }
    }

    fn emit(&mut self, byte: u8) -> Result<(), ReferenceValueCleanerError> {
        if self.output_len == self.output.len() {
            return Err(ReferenceValueCleanerError::InternalOutputOverflow);
        }
        self.output[self.output_len] = byte;
        self.output_len += 1;
        self.receipt.output_bytes = self
            .receipt
            .output_bytes
            .checked_add(1)
            .ok_or(ReferenceValueCleanerError::CounterOverflow)?;
        self.receipt.maximum_output_chunk_bytes =
            self.receipt.maximum_output_chunk_bytes.max(self.output_len);
        Ok(())
    }
}

impl Default for ReferenceValueBodyCleaner {
    fn default() -> Self {
        Self::new()
    }
}

/// First pass for Comrak's ASCII `trim_slice` destination behavior.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DestinationTrimProbe {
    bytes: usize,
    first_non_space: Option<usize>,
    last_non_space_end: usize,
}

impl DestinationTrimProbe {
    /// Consumes one byte of the destination range.
    ///
    /// # Errors
    ///
    /// Returns an error only if the range length overflows `usize`.
    pub fn push(&mut self, byte: u8) -> Result<(), ReferenceValueCleanerError> {
        let start = self.bytes;
        self.bytes = self
            .bytes
            .checked_add(1)
            .ok_or(ReferenceValueCleanerError::CounterOverflow)?;
        if !is_comrak_space(byte) {
            self.first_non_space.get_or_insert(start);
            self.last_non_space_end = self.bytes;
        }
        Ok(())
    }

    #[must_use]
    pub fn finish(self) -> Range<usize> {
        self.first_non_space
            .map_or(0..0, |start| start..self.last_non_space_end)
    }
}

/// Exact body selected by Comrak's `clean_title` before entity/backslash work.
#[must_use]
pub const fn clean_title_body_range(
    len: usize,
    first: Option<u8>,
    last: Option<u8>,
) -> Range<usize> {
    if len >= 2
        && matches!(
            (first, last),
            (Some(b'\''), Some(b'\'')) | (Some(b'"'), Some(b'"')) | (Some(b'('), Some(b')'))
        )
    {
        1..len - 1
    } else {
        0..len
    }
}

enum DecodedEntity {
    Static(&'static str),
    Scalar(char),
}

fn decode_entity(candidate: &[u8]) -> Option<DecodedEntity> {
    if candidate.len() >= 3 && candidate[0] == b'#' {
        return decode_numeric_entity(candidate).map(DecodedEntity::Scalar);
    }
    if candidate.len() < 3 || candidate.last() != Some(&b';') {
        return None;
    }
    let name = std::str::from_utf8(&candidate[..candidate.len() - 1]).ok()?;
    generated::ENTITY_MAP
        .get(name)
        .copied()
        .map(DecodedEntity::Static)
}

fn decode_numeric_entity(candidate: &[u8]) -> Option<char> {
    if candidate.first() != Some(&b'#') || candidate.last() != Some(&b';') {
        return None;
    }
    let (radix, digits, maximum_digits) = if matches!(candidate.get(1), Some(b'x' | b'X')) {
        (16_u32, &candidate[2..candidate.len() - 1], 6_usize)
    } else {
        (10_u32, &candidate[1..candidate.len() - 1], 7_usize)
    };
    if digits.is_empty() || digits.len() > maximum_digits {
        return None;
    }
    let mut codepoint = 0_u32;
    for &byte in digits {
        let digit = match radix {
            10 if byte.is_ascii_digit() => u32::from(byte - b'0'),
            16 if byte.is_ascii_hexdigit() => u32::from((byte | 32) % 39 - 9),
            _ => return None,
        };
        codepoint = codepoint
            .saturating_mul(radix)
            .saturating_add(digit)
            .min(0x11_0000);
    }
    if codepoint == 0 || (0xD800..=0xE000).contains(&codepoint) || codepoint >= 0x11_0000 {
        codepoint = 0xFFFD;
    }
    char::from_u32(codepoint).or(Some('\u{FFFD}'))
}

const fn is_comrak_space(byte: u8) -> bool {
    matches!(byte, b'\t' | b'\n' | b'\r' | b' ')
}

const fn is_ascii_punctuation(byte: u8) -> bool {
    matches!(byte, b'!'..=b'/' | b':'..=b'@' | b'['..=b'`' | b'{'..=b'~')
}

#[cfg(test)]
mod tests {
    use super::*;
    use comrak::Arena;
    use comrak::nodes::NodeValue;
    use comrak::{Options, parse_document};
    use entities::ENTITIES;

    fn clean_body(input: &[u8]) -> (Vec<u8>, ReferenceValueCleanerReceipt) {
        let mut cleaner = ReferenceValueBodyCleaner::new();
        let mut output = Vec::new();
        for &byte in input {
            cleaner.offer_byte(byte).unwrap();
            loop {
                match cleaner.poll().unwrap() {
                    ReferenceValueCleanerStatus::Progress => {}
                    ReferenceValueCleanerStatus::NeedInput => break,
                    ReferenceValueCleanerStatus::OutputReady => {
                        output.extend_from_slice(cleaner.take_output().unwrap().bytes());
                    }
                    ReferenceValueCleanerStatus::Complete => panic!("completed before EOF"),
                }
            }
        }
        cleaner.finish_input().unwrap();
        loop {
            match cleaner.poll().unwrap() {
                ReferenceValueCleanerStatus::Progress | ReferenceValueCleanerStatus::NeedInput => {}
                ReferenceValueCleanerStatus::OutputReady => {
                    output.extend_from_slice(cleaner.take_output().unwrap().bytes());
                }
                ReferenceValueCleanerStatus::Complete => break,
            }
        }
        (output, cleaner.receipt())
    }

    fn stock_reference(destination: &str, title: &str) -> (String, String) {
        let markdown = format!("[x]: <{destination}> \"{title}\"\n\n[x][]\n");
        let arena = Arena::new();
        let root = parse_document(&arena, &markdown, &Options::default());
        root.descendants()
            .find_map(|node| match &node.data().value {
                NodeValue::Link(link) => Some((link.url.clone(), link.title.clone())),
                _ => None,
            })
            .expect("valid reference definition resolves")
    }

    fn clean_destination(destination: &str) -> String {
        let mut trim = DestinationTrimProbe::default();
        for &byte in destination.as_bytes() {
            trim.push(byte).unwrap();
        }
        String::from_utf8(clean_body(&destination.as_bytes()[trim.finish()]).0).unwrap()
    }

    #[test]
    fn streaming_entity_and_backslash_pipeline_matches_edge_cases() {
        for (input, expected) in [
            (b"a&amp;b".as_slice(), b"a&b".as_slice()),
            (b"\\*".as_slice(), b"*".as_slice()),
            (b"\\\\*".as_slice(), b"\\*".as_slice()),
            (b"&x&amp;".as_slice(), b"&x&".as_slice()),
            (b"&#0;".as_slice(), "\u{fffd}".as_bytes()),
            (b"&bsol;*".as_slice(), b"*".as_slice()),
        ] {
            assert_eq!(clean_body(input).0, expected, "input {input:?}");
        }
    }

    #[test]
    fn rejected_second_offer_does_not_replace_the_pending_byte() {
        let mut cleaner = ReferenceValueBodyCleaner::new();
        cleaner.offer_byte(b'a').unwrap();
        assert_eq!(
            cleaner.offer_byte(b'b'),
            Err(ReferenceValueCleanerError::InputAlreadyPending)
        );
        assert_eq!(
            cleaner.poll().unwrap(),
            ReferenceValueCleanerStatus::OutputReady
        );
        assert_eq!(cleaner.take_output().unwrap().bytes(), b"a");
        assert_eq!(cleaner.receipt().input_bytes, 1);
    }

    #[test]
    fn destination_trim_and_title_delimiter_selection_are_exact() {
        let mut trim = DestinationTrimProbe::default();
        for byte in b" \tvalue \r" {
            trim.push(*byte).unwrap();
        }
        assert_eq!(trim.finish(), 2..7);
        assert_eq!(clean_title_body_range(5, Some(b'"'), Some(b'"')), 1..4);
        assert_eq!(clean_title_body_range(5, Some(b'('), Some(b')')), 1..4);
        assert_eq!(clean_title_body_range(5, Some(b'"'), Some(b'\'')), 0..5);
    }

    #[test]
    fn accepted_reference_values_match_pinned_comrak_ast() {
        let fixtures = [
            (" /a&amp;b ", "title &amp; more"),
            (r"/a\\*b", r"a\\*b"),
            ("&#x1F600;", "&#0;"),
            ("&#1114111;", "&#x10FFFF;"),
            ("&#1114112;", "&#x110000;"),
            ("&#55295;", "&#xD800;"),
            ("&#57344;", "&#xE001;"),
            ("&x&amp;", "&bsol;*"),
            ("&a &amp;", "é 😀 e\u{301}"),
            ("", ""),
        ];
        for (destination, title) in fixtures {
            let expected = stock_reference(destination, title);
            let actual_destination = clean_destination(destination);
            let quoted_title = format!("\"{title}\"");
            let title_range = clean_title_body_range(
                quoted_title.len(),
                quoted_title.as_bytes().first().copied(),
                quoted_title.as_bytes().last().copied(),
            );
            let actual_title =
                String::from_utf8(clean_body(&quoted_title.as_bytes()[title_range]).0).unwrap();
            assert_eq!((actual_destination, actual_title), expected);
        }
    }

    #[test]
    fn every_pinned_named_entity_matches_comrak_and_the_expansion_bound() {
        let mut maximum = (1_usize, 1_usize);
        let mut checked = 0_usize;
        for entity in ENTITIES
            .iter()
            .filter(|entity| entity.entity.starts_with('&') && entity.entity.ends_with(';'))
        {
            let expected = stock_reference(entity.entity, "").0;
            let actual = clean_destination(entity.entity);
            assert_eq!(actual, expected, "entity {}", entity.entity);
            if entity.characters.len() * maximum.1 > maximum.0 * entity.entity.len() {
                maximum = (entity.characters.len(), entity.entity.len());
            }
            checked += 1;
        }
        assert!(checked > 2_000);
        assert_eq!(
            maximum,
            (
                MAX_ENTITY_EXPANSION_NUMERATOR,
                MAX_ENTITY_EXPANSION_DENOMINATOR,
            )
        );
    }

    #[test]
    fn randomized_accepted_values_match_pinned_comrak() {
        struct Lcg(u64);

        impl Lcg {
            fn next(&mut self) -> u64 {
                self.0 = self
                    .0
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1_442_695_040_888_963_407);
                self.0
            }

            fn usize(&mut self, ceiling: usize) -> usize {
                usize::try_from(self.next() % u64::try_from(ceiling).unwrap()).unwrap()
            }
        }

        let alphabet = b"&;#xX0123456789 abcXYZ/*_=-!?\\'()[]";
        let mut rng = Lcg(0x5ea1_c1ea);
        for _ in 0..2_000 {
            let len = rng.usize(129);
            let mut value: String = (0..len)
                .map(|_| char::from(alphabet[rng.usize(alphabet.len())]))
                .collect();
            // Keep both the angle destination and quoted title syntactically
            // closed; backslashes elsewhere remain in the randomized corpus.
            if value.ends_with('\\') {
                value.push('a');
            }
            let expected = stock_reference(&value, &value);
            let destination = clean_destination(&value);
            let quoted_title = format!("\"{value}\"");
            let title_range = clean_title_body_range(
                quoted_title.len(),
                quoted_title.as_bytes().first().copied(),
                quoted_title.as_bytes().last().copied(),
            );
            let title =
                String::from_utf8(clean_body(&quoted_title.as_bytes()[title_range]).0).unwrap();
            assert_eq!((destination, title), expected, "source value {value:?}");
        }
    }

    #[test]
    fn scratch_and_output_are_table_bounded_and_expansion_is_pinned() {
        let invalid = format!("&{};", "x".repeat(MAX_ENTITY_CANDIDATE_BYTES));
        let input = format!("{invalid}&amp;");
        let (output, receipt) = clean_body(input.as_bytes());
        assert_eq!(output, format!("{invalid}&").as_bytes());
        assert_eq!(
            receipt.maximum_entity_candidate_bytes,
            MAX_ENTITY_CANDIDATE_BYTES
        );
        assert!(receipt.maximum_output_chunk_bytes <= MAX_CLEAN_OUTPUT_CHUNK_BYTES);
        assert!(
            u128::from(receipt.output_bytes)
                * u128::try_from(MAX_ENTITY_EXPANSION_DENOMINATOR).unwrap()
                <= u128::from(receipt.input_bytes)
                    * u128::try_from(MAX_ENTITY_EXPANSION_NUMERATOR).unwrap()
        );
    }
}
