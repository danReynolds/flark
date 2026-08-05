//! `CommonMark` reference-label facts shared by block and inline recognition.

use caseless::Caseless;

/// `CommonMark` 0.31.2 permits at most 999 Unicode scalar values between the
/// label brackets.
pub const MAX_REFERENCE_LABEL_CODEPOINTS: usize = 999;

/// Maximum UTF-8 bytes in the raw body of a valid reference label.
pub const MAX_RAW_REFERENCE_LABEL_UTF8_BYTES: usize = MAX_REFERENCE_LABEL_CODEPOINTS * 4;

/// Maximum UTF-8 bytes emitted by the pinned Unicode default case-fold table
/// for one valid input scalar. The exhaustive test below locks this to the
/// dependency's exported Unicode table rather than an assumed multiplier.
pub const MAX_CASE_FOLD_UTF8_BYTES_PER_SCALAR: usize = 6;

/// Preflight envelope for one normalized reference-label output allocation.
pub const MAX_NORMALIZED_REFERENCE_LABEL_BYTES: usize =
    MAX_REFERENCE_LABEL_CODEPOINTS * MAX_CASE_FOLD_UTF8_BYTES_PER_SCALAR;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReferenceLabelAccumulatorError {
    InvalidRawCodepointContribution(u8),
    TooLong,
}

/// Resumable `CommonMark` label normalization and raw-source length state.
///
/// Callers project each logical scalar with its authenticated raw-source
/// scalar contribution. In particular, a canonical LF backed by CRLF has a
/// contribution of two.
#[derive(Debug)]
pub struct ReferenceLabelAccumulator {
    normalized: String,
    pending_space: bool,
    raw_codepoints: usize,
}

impl ReferenceLabelAccumulator {
    #[must_use]
    pub fn new() -> Self {
        Self {
            normalized: String::new(),
            pending_space: false,
            raw_codepoints: 0,
        }
    }

    /// Allocate the complete proved output envelope before resumable polling.
    #[must_use]
    pub fn new_preflighted() -> Self {
        Self {
            normalized: String::with_capacity(MAX_NORMALIZED_REFERENCE_LABEL_BYTES),
            pending_space: false,
            raw_codepoints: 0,
        }
    }

    /// Preflight a source-range-sized output hint without reserving the global
    /// maximum for every small inline candidate.
    #[must_use]
    pub fn with_source_byte_hint(source_bytes: usize) -> Self {
        let capacity = source_bytes
            .saturating_mul(MAX_CASE_FOLD_UTF8_BYTES_PER_SCALAR)
            .min(MAX_NORMALIZED_REFERENCE_LABEL_BYTES);
        Self {
            normalized: String::with_capacity(capacity),
            pending_space: false,
            raw_codepoints: 0,
        }
    }

    /// Adds one authenticated logical scalar to this label.
    ///
    /// # Errors
    ///
    /// Returns an error when the raw-source contribution is invalid or would
    /// exceed the selected profile's 999-scalar limit.
    pub fn push(
        &mut self,
        ch: char,
        raw_codepoint_contribution: u8,
    ) -> Result<(), ReferenceLabelAccumulatorError> {
        if raw_codepoint_contribution > 2 {
            return Err(
                ReferenceLabelAccumulatorError::InvalidRawCodepointContribution(
                    raw_codepoint_contribution,
                ),
            );
        }
        self.raw_codepoints = self
            .raw_codepoints
            .checked_add(usize::from(raw_codepoint_contribution))
            .ok_or(ReferenceLabelAccumulatorError::TooLong)?;
        if self.raw_codepoints > MAX_REFERENCE_LABEL_CODEPOINTS {
            return Err(ReferenceLabelAccumulatorError::TooLong);
        }
        if is_reference_label_whitespace(ch) {
            self.pending_space = !self.normalized.is_empty();
            return Ok(());
        }
        if self.pending_space {
            self.normalized.push(' ');
            self.pending_space = false;
        }
        self.normalized
            .extend(std::iter::once(ch).default_case_fold());
        debug_assert!(self.normalized.len() <= MAX_NORMALIZED_REFERENCE_LABEL_BYTES);
        Ok(())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.normalized
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.normalized.is_empty()
    }

    #[must_use]
    pub fn allocated_bytes(&self) -> usize {
        self.normalized.capacity()
    }

    #[must_use]
    pub fn into_normalized(self) -> String {
        self.normalized
    }
}

impl Default for ReferenceLabelAccumulator {
    fn default() -> Self {
        Self::new()
    }
}

/// Normalize an already-recognized `CommonMark` reference label.
///
/// Only spaces, tabs, and line endings are collapsed. Other Unicode
/// whitespace remains significant. Unicode default case folding happens only
/// after that whitespace transform.
#[must_use]
pub fn normalize_reference_label(label: &str) -> String {
    let mut collapsed = String::with_capacity(label.len());
    let mut pending_space = false;
    for ch in label.trim_matches(is_reference_label_whitespace).chars() {
        if is_reference_label_whitespace(ch) {
            pending_space = !collapsed.is_empty();
        } else {
            if pending_space {
                collapsed.push(' ');
                pending_space = false;
            }
            collapsed.push(ch);
        }
    }
    caseless::default_case_fold_str(&collapsed)
}

/// Whether the source label body satisfies `CommonMark`'s scalar-value bound.
#[must_use]
pub fn reference_label_length_is_valid(label: &str) -> bool {
    label
        .chars()
        .take(MAX_REFERENCE_LABEL_CODEPOINTS + 1)
        .count()
        <= MAX_REFERENCE_LABEL_CODEPOINTS
}

#[must_use]
pub const fn is_reference_label_whitespace(ch: char) -> bool {
    matches!(ch, ' ' | '\t' | '\r' | '\n')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_block_inline_normalization_fixtures() {
        for (raw, expected) in [
            ("  Foo\t\r\n BAR  ", "foo bar"),
            ("Straẞe", "strasse"),
            ("Foo\u{a0} BAR", "foo\u{a0} bar"),
            ("Foo\u{2003}BAR", "foo\u{2003}bar"),
            ("Foo\u{b}BAR", "foo\u{b}bar"),
            ("Foo\u{c}BAR", "foo\u{c}bar"),
        ] {
            assert_eq!(normalize_reference_label(raw), expected, "raw={raw:?}");
        }
    }

    #[test]
    fn length_is_unicode_scalar_based() {
        assert!(reference_label_length_is_valid(&"a".repeat(999)));
        assert!(!reference_label_length_is_valid(&"a".repeat(1000)));
        assert!(reference_label_length_is_valid(&"é".repeat(999)));
        assert!(!reference_label_length_is_valid(&"é".repeat(1000)));
    }

    #[test]
    fn pinned_case_fold_expansion_sets_the_output_envelope() {
        let mut maximum_bytes = 0;
        let mut maximum_scalars = 0;
        let mut maximum_byte_scalar = '\0';
        let mut maximum_scalar_scalar = '\0';
        for value in 0..=0x10_ffff {
            let Some(ch) = char::from_u32(value) else {
                continue;
            };
            let folded = caseless::default_case_fold_str(ch.encode_utf8(&mut [0; 4]));
            if folded.len() > maximum_bytes {
                maximum_bytes = folded.len();
                maximum_byte_scalar = ch;
            }
            let scalar_count = folded.chars().count();
            if scalar_count > maximum_scalars {
                maximum_scalars = scalar_count;
                maximum_scalar_scalar = ch;
            }
        }
        assert_eq!(
            maximum_bytes,
            MAX_CASE_FOLD_UTF8_BYTES_PER_SCALAR,
            "pinned caseless Unicode {:?}: max byte scalar U+{:04X}; max scalar expansion {} at U+{:04X}",
            caseless::UNICODE_VERSION,
            u32::from(maximum_byte_scalar),
            maximum_scalars,
            u32::from(maximum_scalar_scalar),
        );
        assert_eq!(maximum_scalars, 3);
    }

    #[test]
    fn resumable_accumulator_uses_raw_crlf_metric_and_shared_normalization() {
        let mut accumulator = ReferenceLabelAccumulator::new_preflighted();
        accumulator.push('S', 1).unwrap();
        accumulator.push('t', 1).unwrap();
        accumulator.push('r', 1).unwrap();
        accumulator.push('a', 1).unwrap();
        accumulator.push('ẞ', 1).unwrap();
        accumulator.push('e', 1).unwrap();
        accumulator.push('\n', 2).unwrap();
        accumulator.push('\u{a0}', 1).unwrap();
        assert_eq!(accumulator.as_str(), "strasse \u{a0}");
        assert_eq!(
            accumulator.allocated_bytes(),
            MAX_NORMALIZED_REFERENCE_LABEL_BYTES
        );
    }
}
