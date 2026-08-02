//! Bounded streaming transforms for parser-authenticated reference values.
//!
//! Recognition and source cuts remain owned by the exact block controller.
//! This module implements only Comrak's selected `clean_url`/`clean_title`
//! transforms: destination trimming or title-delimiter removal, pinned entity
//! decoding, then ASCII-punctuation backslash removal.

use std::fmt;
use std::ops::Range;

use comrak::block_spine_facade;

/// Comrak examines at most 32 bytes after `&` for one entity.
pub(crate) const MAX_ENTITY_CANDIDATE_BYTES: usize = 32;
/// One decoded entity plus a pending literal backslash fits this output.
pub(crate) const MAX_CLEAN_OUTPUT_CHUNK_BYTES: usize = 16;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct ReferenceValueCleanerReceipt {
    pub(crate) polls: u64,
    pub(crate) input_bytes: u64,
    pub(crate) replayed_bytes: u64,
    pub(crate) output_bytes: u64,
    pub(crate) entities_decoded: u64,
    pub(crate) invalid_entity_fallbacks: u64,
    pub(crate) backslashes_removed: u64,
    pub(crate) maximum_entity_candidate_bytes: usize,
    pub(crate) maximum_output_chunk_bytes: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ReferenceValueCleanerStatus {
    Progress,
    NeedInput,
    OutputReady,
    Complete,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ReferenceValueCleanerError {
    InputAlreadyPending,
    OutputNotConsumed,
    InputAlreadyFinished,
    OutputNotReady,
    CounterOverflow,
    InternalOutputOverflow,
    InvalidUtf8,
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
            Self::InvalidUtf8 => "reference-value entity candidate is not valid UTF-8",
        })
    }
}

impl std::error::Error for ReferenceValueCleanerError {}

#[must_use = "cleaned bytes must be consumed by the persistent sink"]
pub(crate) struct CleanReferenceValueChunk {
    bytes: [u8; MAX_CLEAN_OUTPUT_CHUNK_BYTES],
    len: usize,
}

impl CleanReferenceValueChunk {
    #[must_use]
    pub(crate) fn bytes(&self) -> &[u8] {
        &self.bytes[..self.len]
    }
}

/// Incremental entity decoding followed by Comrak's backslash unescape.
#[derive(Debug)]
pub(crate) struct ReferenceValueBodyCleaner {
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum InputPhase {
    Accepting,
    Finished,
    Complete,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EntityState {
    Idle,
    Candidate { len: usize },
}

impl ReferenceValueBodyCleaner {
    #[must_use]
    pub(crate) const fn new() -> Self {
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

    pub(crate) fn offer_byte(&mut self, byte: u8) -> Result<(), ReferenceValueCleanerError> {
        if self.phase != InputPhase::Accepting {
            return Err(ReferenceValueCleanerError::InputAlreadyFinished);
        }
        if self.output_len != 0 {
            return Err(ReferenceValueCleanerError::OutputNotConsumed);
        }
        if self.pending_input.is_some() {
            return Err(ReferenceValueCleanerError::InputAlreadyPending);
        }
        self.receipt.input_bytes = self
            .receipt
            .input_bytes
            .checked_add(1)
            .ok_or(ReferenceValueCleanerError::CounterOverflow)?;
        self.pending_input = Some(byte);
        Ok(())
    }

    pub(crate) fn finish_input(&mut self) -> Result<(), ReferenceValueCleanerError> {
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

    /// Advances exactly one input, replay, fallback, or finalization step.
    pub(crate) fn poll(
        &mut self,
    ) -> Result<ReferenceValueCleanerStatus, ReferenceValueCleanerError> {
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

    pub(crate) fn take_output(
        &mut self,
    ) -> Result<CleanReferenceValueChunk, ReferenceValueCleanerError> {
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
    pub(crate) const fn receipt(&self) -> ReferenceValueCleanerReceipt {
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
            let candidate = std::str::from_utf8(&self.entity[..len])
                .map_err(|_| ReferenceValueCleanerError::InvalidUtf8)?;
            let mut decoded = [0_u8; MAX_CLEAN_OUTPUT_CHUNK_BYTES];
            if let Some(decoded_len) =
                block_spine_facade::decode_reference_entity(candidate, &mut decoded)
            {
                self.entity_state = EntityState::Idle;
                self.receipt.entities_decoded = self
                    .receipt
                    .entities_decoded
                    .checked_add(1)
                    .ok_or(ReferenceValueCleanerError::CounterOverflow)?;
                for byte in &decoded[..decoded_len] {
                    self.feed_unescape(*byte)?;
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

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct DestinationTrimProbe {
    bytes: usize,
    first_non_space: Option<usize>,
    last_non_space_end: usize,
}

impl DestinationTrimProbe {
    pub(crate) fn push(&mut self, byte: u8) -> Result<(), ReferenceValueCleanerError> {
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
    pub(crate) fn finish(self) -> Range<usize> {
        self.first_non_space
            .map_or(0..0, |start| start..self.last_non_space_end)
    }
}

#[must_use]
pub(crate) const fn clean_title_body_range(
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

const fn is_comrak_space(byte: u8) -> bool {
    matches!(byte, b'\t' | b'\n' | b'\r' | b' ')
}

const fn is_ascii_punctuation(byte: u8) -> bool {
    matches!(byte, b'!'..=b'/' | b':'..=b'@' | b'['..=b'`' | b'{'..=b'~')
}

#[cfg(test)]
mod tests {
    use super::*;
    use entities::ENTITIES;

    fn clean_body(input: &[u8]) -> (Vec<u8>, ReferenceValueCleanerReceipt) {
        let mut cleaner = ReferenceValueBodyCleaner::new();
        let mut output = Vec::new();
        for byte in input {
            cleaner.offer_byte(*byte).unwrap();
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

    fn clean_destination(input: &str) -> String {
        let mut probe = DestinationTrimProbe::default();
        for byte in input.bytes() {
            probe.push(byte).unwrap();
        }
        String::from_utf8(clean_body(&input.as_bytes()[probe.finish()]).0).unwrap()
    }

    fn clean_title(input: &str) -> String {
        let body = clean_title_body_range(
            input.len(),
            input.as_bytes().first().copied(),
            input.as_bytes().last().copied(),
        );
        String::from_utf8(clean_body(&input.as_bytes()[body]).0).unwrap()
    }

    #[test]
    fn streaming_cleaner_matches_pinned_comrak_edges() {
        for (destination, title) in [
            (" /a&amp;b\\* \r", "\"a&amp;b\\*\""),
            ("&#x1F600;", "'&#0;'"),
            ("&x&amp;", "(&bsol;*)"),
            (" 	\r", "\"\""),
            ("é 😀 e\u{301}", "'é 😀 e\u{301}'"),
        ] {
            assert_eq!(
                clean_destination(destination),
                block_spine_facade::clean_reference_destination(destination).unwrap()
            );
            assert_eq!(
                clean_title(title),
                block_spine_facade::clean_reference_title(title).unwrap()
            );
        }
    }

    #[test]
    fn randomized_values_match_the_pinned_atomic_oracle() {
        let alphabet = b"&;#xX0123456789 abcXYZ/*_=-!?\\'()[]\t\r\n";
        let mut state = 0x5ea1_c1ea_u64;
        for _ in 0..10_000 {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            let len = usize::try_from(state % 257).unwrap();
            let mut value = String::new();
            for _ in 0..len {
                state = state
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1_442_695_040_888_963_407);
                value.push(char::from(
                    alphabet[usize::try_from(state).unwrap() % alphabet.len()],
                ));
            }
            assert_eq!(
                clean_destination(&value),
                block_spine_facade::clean_reference_destination(&value).unwrap(),
                "destination {value:?}"
            );
            let title = match state % 3 {
                0 => format!("\"{value}\""),
                1 => format!("'{value}'"),
                _ => format!("({value})"),
            };
            assert_eq!(
                clean_title(&title),
                block_spine_facade::clean_reference_title(&title).unwrap(),
                "title {title:?}"
            );
        }
    }

    #[test]
    fn every_pinned_semicolon_entity_matches_comrak() {
        let mut checked = 0;
        for entity in ENTITIES
            .iter()
            .filter(|entity| entity.entity.starts_with('&') && entity.entity.ends_with(';'))
        {
            assert_eq!(
                clean_destination(entity.entity),
                block_spine_facade::clean_reference_destination(entity.entity).unwrap(),
                "entity {:?}",
                entity.entity,
            );
            checked += 1;
        }
        assert!(checked > 2_000, "pinned entity table unexpectedly shrank");
    }

    #[test]
    fn scratch_state_is_strictly_bounded() {
        let invalid = format!("&{};&amp;", "x".repeat(MAX_ENTITY_CANDIDATE_BYTES));
        let (_, receipt) = clean_body(invalid.as_bytes());
        assert!(receipt.maximum_entity_candidate_bytes <= MAX_ENTITY_CANDIDATE_BYTES);
        assert!(receipt.maximum_output_chunk_bytes <= MAX_CLEAN_OUTPUT_CHUNK_BYTES);
    }
}
