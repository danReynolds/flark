//! One Flark-owned CommonMark reference-label authority shared by block and
//! inline recognition.

use caseless::Caseless;

/// CommonMark 0.31.2 permits at most 999 Unicode scalar values between label
/// brackets.
pub(crate) const MAX_REFERENCE_LABEL_CODEPOINTS: usize = 999;

/// Exhaustively pinned against `caseless` 0.2.2 / Unicode 16 in the tests.
pub(crate) const MAX_CASE_FOLD_UTF8_BYTES_PER_SCALAR: usize = 6;
pub(crate) const MAX_NORMALIZED_REFERENCE_LABEL_BYTES: usize =
    MAX_REFERENCE_LABEL_CODEPOINTS * MAX_CASE_FOLD_UTF8_BYTES_PER_SCALAR;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ReferenceLabelAccumulatorError {
    InvalidRawCodepointContribution(u8),
    TooLong,
}

/// Resumable normalization state for logical-projection cursors. The raw
/// contribution is two only for one canonical LF backed by source CRLF.
#[derive(Debug)]
pub(crate) struct ReferenceLabelAccumulator {
    normalized: String,
    pending_space: bool,
    raw_codepoints: usize,
}

impl ReferenceLabelAccumulator {
    #[must_use]
    pub(crate) fn with_source_byte_hint(source_bytes: usize) -> Self {
        let capacity = source_bytes
            .saturating_mul(MAX_CASE_FOLD_UTF8_BYTES_PER_SCALAR)
            .min(MAX_NORMALIZED_REFERENCE_LABEL_BYTES);
        Self {
            normalized: String::with_capacity(capacity),
            pending_space: false,
            raw_codepoints: 0,
        }
    }

    pub(crate) fn push(
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
    #[cfg(test)]
    pub(crate) fn is_empty(&self) -> bool {
        self.normalized.is_empty()
    }

    #[must_use]
    pub(crate) fn into_normalized(self) -> String {
        self.normalized
    }
}

/// Normalizes one already-recognized source label with the exact selected
/// profile: only space, tab, CR, and LF collapse; all other Unicode
/// whitespace remains significant.
#[must_use]
#[cfg(test)]
pub(crate) fn normalize_reference_label(label: &str) -> String {
    let mut accumulator = ReferenceLabelAccumulator::with_source_byte_hint(label.len());
    for ch in label.chars() {
        // This helper receives physical source, so CR and LF each contribute
        // one raw scalar. Logical cursors use `push` directly for CRLF.
        if accumulator.push(ch, 1).is_err() {
            return String::new();
        }
    }
    if accumulator.is_empty() {
        String::new()
    } else {
        accumulator.into_normalized()
    }
}

#[must_use]
#[cfg(test)]
pub(crate) fn reference_label_length_is_valid(label: &str) -> bool {
    label
        .chars()
        .take(MAX_REFERENCE_LABEL_CODEPOINTS + 1)
        .count()
        <= MAX_REFERENCE_LABEL_CODEPOINTS
}

#[must_use]
pub(crate) const fn is_reference_label_whitespace(ch: char) -> bool {
    matches!(ch, ' ' | '\t' | '\r' | '\n')
}

#[cfg(test)]
mod tests {
    use super::*;
    use flark_engine::parser_internal::{
        M11ReferenceJournal, M11ReferenceJournalOccurrence, M11ReferenceJournalRange,
        M11ReferenceJournalStatus, M11ReferenceResolution, M11ReferenceResolver,
    };
    use flark_engine::{DocumentRuntime, DocumentRuntimeConfig};

    #[test]
    fn normalization_uses_only_commonmark_whitespace_and_full_case_fold() {
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
        for ch in ['a', 'é', '界', '🦀'] {
            assert!(reference_label_length_is_valid(&ch.to_string().repeat(999)));
            assert!(!reference_label_length_is_valid(
                &ch.to_string().repeat(1000)
            ));
        }
    }

    #[test]
    fn case_fold_expansion_fits_the_pinned_envelope() {
        let mut maximum_bytes = 0;
        let mut maximum_scalars = 0;
        for value in 0..=0x10_ffff {
            let Some(ch) = char::from_u32(value) else {
                continue;
            };
            let folded = caseless::default_case_fold_str(ch.encode_utf8(&mut [0; 4]));
            maximum_bytes = maximum_bytes.max(folded.len());
            maximum_scalars = maximum_scalars.max(folded.chars().count());
        }
        assert_eq!(maximum_bytes, MAX_CASE_FOLD_UTF8_BYTES_PER_SCALAR);
        assert_eq!(maximum_scalars, 3);
    }

    #[test]
    fn maximum_unicode_case_fold_expansion_survives_live_journal_resolution() {
        // U+0390 folds to U+03B9 + U+0308 + U+0301: six UTF-8 bytes for one
        // valid raw scalar. Repeating it to CommonMark's 999-scalar limit
        // reaches the complete derived winner-key envelope exactly.
        let raw_scalar = '\u{0390}';
        let raw_label = raw_scalar
            .to_string()
            .repeat(MAX_REFERENCE_LABEL_CODEPOINTS);
        let scalar_fold = normalize_reference_label(&raw_scalar.to_string());
        assert_eq!(scalar_fold, "\u{03b9}\u{0308}\u{0301}");
        assert_eq!(scalar_fold.len(), MAX_CASE_FOLD_UTF8_BYTES_PER_SCALAR);
        assert!(reference_label_length_is_valid(&raw_label));

        let normalized = normalize_reference_label(&raw_label);
        assert_eq!(normalized.len(), MAX_NORMALIZED_REFERENCE_LABEL_BYTES);
        assert_eq!(normalized.len(), 5_994);

        let destination = "/expanded";
        let definition = format!("[{raw_label}]: {destination}");
        let label_byte_start = 1_usize;
        let label_byte_end = label_byte_start + raw_label.len();
        let destination_byte_start = label_byte_end + "]: ".len();
        let destination_byte_end = destination_byte_start + destination.len();
        assert_eq!(
            &definition[destination_byte_start..destination_byte_end],
            destination
        );

        let label_utf16_start = 1_usize;
        let label_utf16_end = label_utf16_start + raw_label.encode_utf16().count();
        let destination_utf16_start = label_utf16_end + "]: ".encode_utf16().count();
        let destination_utf16_end = destination_utf16_start + destination.encode_utf16().count();
        let range = |start: usize, end: usize| {
            u64::try_from(start).expect("test range start")
                ..u64::try_from(end).expect("test range end")
        };

        let mut runtime =
            DocumentRuntime::new(&definition, DocumentRuntimeConfig::default()).expect("runtime");
        let source = runtime.current_source_version().expect("source");
        let mut journal =
            M11ReferenceJournal::new(&mut runtime, source, 1).expect("reference journal");
        journal
            .offer_occurrence(
                &runtime,
                M11ReferenceJournalOccurrence::new(
                    M11ReferenceJournalRange::new(
                        range(0, definition.len()),
                        range(0, definition.encode_utf16().count()),
                    ),
                    M11ReferenceJournalRange::new(
                        range(label_byte_start, label_byte_end),
                        range(label_utf16_start, label_utf16_end),
                    ),
                    M11ReferenceJournalRange::new(
                        range(destination_byte_start, destination_byte_end),
                        range(destination_utf16_start, destination_utf16_end),
                    ),
                    None,
                    normalized.as_bytes(),
                    destination.as_bytes(),
                    None,
                ),
            )
            .expect("offer expanded occurrence");
        loop {
            let poll = journal.poll(&mut runtime, 1).expect("build live journal");
            if poll.status() == M11ReferenceJournalStatus::NeedsInput {
                break;
            }
            assert_eq!(poll.status(), M11ReferenceJournalStatus::Pending);
        }
        journal
            .finish_input(&runtime)
            .expect("finish journal input");
        loop {
            let poll = journal.poll(&mut runtime, 1).expect("seal live journal");
            if poll.status() == M11ReferenceJournalStatus::Complete {
                break;
            }
            assert_eq!(poll.status(), M11ReferenceJournalStatus::Pending);
        }
        let mut root = journal.take_root().expect("live journal root");
        let resolver = M11ReferenceResolver::from_live_reference_journal(&runtime, &root)
            .expect("live resolver");

        let resolved = resolver
            .resolve(&runtime, &normalized, 64)
            .expect("expanded lookup");
        let M11ReferenceResolution::Resolved(resolved) = resolved else {
            panic!("maximum valid expanded label did not resolve");
        };
        assert_eq!(resolved.definition_ordinal(), 0);
        assert_eq!(resolved.cooked_destination(), destination);
        assert_eq!(
            resolved.destination_source(),
            &range(destination_byte_start, destination_byte_end)
        );

        let near_miss = &normalized[..normalized.len() - scalar_fold.len()];
        assert_eq!(
            resolver
                .resolve(&runtime, near_miss, 64)
                .expect("near-bound miss"),
            M11ReferenceResolution::Missing
        );

        drop(resolver);
        root.begin_release(&mut runtime)
            .expect("begin root release");
        while !root
            .poll_release(&mut runtime, 1)
            .expect("poll root release")
            .complete()
        {}
        runtime.begin_close().expect("begin runtime close");
        while !runtime.poll_close(64).expect("poll runtime close").complete {}
    }

    #[test]
    fn logical_crlf_counts_twice_but_collapses_once() {
        let mut accumulator = ReferenceLabelAccumulator::with_source_byte_hint(16);
        accumulator.push('a', 1).unwrap();
        accumulator.push('\n', 2).unwrap();
        accumulator.push('ẞ', 1).unwrap();
        assert_eq!(accumulator.into_normalized(), "a ss");
    }
}
