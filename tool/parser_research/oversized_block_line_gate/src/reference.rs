//! Resumable structural recognition for one leading reference definition.
//!
//! This is a byte-for-byte state-machine port of the donor facade's
//! `link_label` + `spnl` + `manual_scan_link_url` + `link_title` sequence.
//! URL/title cleaning is intentionally a separate, source-backed transform.

use std::ops::Range;

use serde::{Deserialize, Serialize};

use crate::{CancellationToken, MAX_REFERENCE_LABEL_BYTES, Poll, ScanReceipt};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReferenceDefinitionShape {
    pub source: Range<usize>,
    pub label: Range<usize>,
    pub destination: Range<usize>,
    /// Includes the title delimiters, as does the donor facade.
    pub title: Option<Range<usize>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum Phase {
    Label,
    Colon,
    BeforeDestination,
    AngleDestination,
    AngleClosed,
    BareDestination,
    AfterDestination,
    QuotedTitle,
    ParenTitle,
    AfterTitle,
    AfterTitleCr,
    Done,
}

/// Recognizes one definition beginning at byte zero. Repeated leading
/// definitions are handled by starting another job at the returned source end.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReferencePrefixJob {
    cursor: usize,
    phase: Phase,

    label_start: usize,
    label_end: usize,
    label_length: usize,
    label_backslash: bool,

    destination_start: usize,
    destination_end: usize,
    destination_bytes: usize,
    bare_depth: u8,
    bare_backslash: bool,
    angle_backslash: bool,

    before_destination_newline: bool,
    before_destination_cr: bool,

    after_destination_separator: bool,
    after_destination_newline: bool,
    after_destination_cr: bool,
    fallback_source_end: Option<usize>,

    title_start: Option<usize>,
    title_end: Option<usize>,
    title_closer: u8,
    title_backslash: bool,

    done: bool,
    result: Option<ReferenceDefinitionShape>,
    receipt: ScanReceipt,
}

impl ReferencePrefixJob {
    pub fn new() -> Self {
        Self {
            cursor: 0,
            phase: Phase::Label,
            label_start: 1,
            label_end: 0,
            label_length: 0,
            label_backslash: false,
            destination_start: 0,
            destination_end: 0,
            destination_bytes: 0,
            bare_depth: 0,
            bare_backslash: false,
            angle_backslash: false,
            before_destination_newline: false,
            before_destination_cr: false,
            after_destination_separator: false,
            after_destination_newline: false,
            after_destination_cr: false,
            fallback_source_end: None,
            title_start: None,
            title_end: None,
            title_closer: 0,
            title_backslash: false,
            done: false,
            result: None,
            receipt: ScanReceipt::default(),
        }
    }

    pub const fn receipt(&self) -> ScanReceipt {
        self.receipt
    }

    pub fn poll(
        &mut self,
        input: &[u8],
        fuel: usize,
        cancellation: &CancellationToken,
    ) -> Poll<Option<ReferenceDefinitionShape>> {
        assert!(fuel > 0);
        if cancellation.is_cancelled() {
            self.receipt.record_poll(0);
            return Poll::Cancelled { inspected: 0 };
        }

        let mut inspected = 0;
        while !self.done && inspected < fuel {
            if self.cursor == input.len() {
                self.finish_at_eof(input);
                break;
            }
            let index = self.cursor;
            let byte = input[index];
            self.cursor += 1;
            inspected += 1;
            self.consume(input, index, byte);
        }

        self.receipt.record_poll(inspected);
        if cancellation.is_cancelled() {
            Poll::Cancelled { inspected }
        } else if self.done {
            Poll::Ready {
                value: self.result.clone(),
                inspected,
            }
        } else {
            Poll::Pending { inspected }
        }
    }

    fn consume(&mut self, input: &[u8], index: usize, byte: u8) {
        match self.phase {
            Phase::Label => self.consume_label(input, index, byte),
            Phase::Colon => {
                if byte == b':' {
                    self.phase = Phase::BeforeDestination;
                } else {
                    self.reject();
                }
            }
            Phase::BeforeDestination => self.consume_before_destination(index, byte),
            Phase::AngleDestination => self.consume_angle_destination(index, byte),
            Phase::AngleClosed => {
                self.phase = Phase::AfterDestination;
                self.consume_after_destination(index, byte);
            }
            Phase::BareDestination => self.consume_bare_destination(index, byte),
            Phase::AfterDestination => self.consume_after_destination(index, byte),
            Phase::QuotedTitle => self.consume_quoted_title(index, byte),
            Phase::ParenTitle => self.consume_paren_title(index, byte),
            Phase::AfterTitle => self.consume_after_title(index, byte),
            Phase::AfterTitleCr => {
                let end = if byte == b'\n' { self.cursor } else { index };
                self.accept(input, end);
            }
            Phase::Done => unreachable!("completed job returned before consume"),
        }
    }

    fn consume_label(&mut self, input: &[u8], index: usize, byte: u8) {
        if index == 0 {
            if byte != b'[' {
                self.reject();
            }
            return;
        }

        if self.label_backslash {
            self.label_backslash = false;
            if is_ascii_punctuation(byte) {
                self.label_length += 1;
                self.enforce_label_limit();
                return;
            }
        }

        match byte {
            b']' => {
                let label = trim_cmark(input, self.label_start..index);
                if label.is_empty() {
                    self.reject();
                } else {
                    self.label_start = label.start;
                    self.label_end = label.end;
                    self.phase = Phase::Colon;
                }
            }
            b'[' => self.reject(),
            b'\\' => {
                self.label_length += 1;
                self.label_backslash = true;
                self.enforce_label_limit();
            }
            _ => {
                self.label_length += 1;
                self.enforce_label_limit();
            }
        }
    }

    fn enforce_label_limit(&mut self) {
        if self.label_length > MAX_REFERENCE_LABEL_BYTES {
            self.reject();
        }
    }

    fn consume_before_destination(&mut self, index: usize, byte: u8) {
        if self.before_destination_cr {
            self.before_destination_cr = false;
            if byte == b'\n' {
                return;
            }
            self.start_destination(index, byte);
            return;
        }

        if is_space_or_tab(byte) {
            return;
        }
        if !self.before_destination_newline && matches!(byte, b'\r' | b'\n') {
            self.before_destination_newline = true;
            self.before_destination_cr = byte == b'\r';
            return;
        }
        self.start_destination(index, byte);
    }

    fn start_destination(&mut self, index: usize, byte: u8) {
        self.destination_start = index;
        if byte == b'<' {
            self.destination_start = index + 1;
            self.phase = Phase::AngleDestination;
        } else {
            self.phase = Phase::BareDestination;
            self.consume_bare_destination(index, byte);
        }
    }

    fn consume_angle_destination(&mut self, index: usize, byte: u8) {
        if self.angle_backslash {
            self.angle_backslash = false;
            return;
        }
        match byte {
            b'\\' => self.angle_backslash = true,
            b'>' => {
                self.destination_end = index;
                // The donor's manual angle scanner rejects `>` at EOF. Wait
                // for one following byte, then feed it to SPNL without rewind.
                self.phase = Phase::AngleClosed;
            }
            b'\r' | b'\n' | b'<' => self.reject(),
            _ => {}
        }
    }

    fn consume_bare_destination(&mut self, index: usize, byte: u8) {
        if self.bare_backslash {
            self.bare_backslash = false;
            if is_ascii_punctuation(byte) {
                self.destination_bytes += 1;
                return;
            }
        }

        if byte == b'\\' {
            self.destination_bytes += 1;
            self.bare_backslash = true;
            return;
        }
        if byte == b'(' {
            self.destination_bytes += 1;
            self.bare_depth = self.bare_depth.saturating_add(1);
            if self.bare_depth > 32 {
                self.reject();
            }
            return;
        }
        if byte == b')' {
            if self.bare_depth > 0 {
                self.destination_bytes += 1;
                self.bare_depth -= 1;
            } else {
                self.end_bare_destination(index, byte);
            }
            return;
        }
        if is_url_space(byte) || (byte.is_ascii_control() && byte != 0) {
            self.end_bare_destination(index, byte);
            return;
        }
        self.destination_bytes += 1;
    }

    fn end_bare_destination(&mut self, index: usize, delimiter: u8) {
        if self.destination_bytes == 0 || self.bare_depth != 0 {
            self.reject();
            return;
        }
        self.destination_end = index;
        self.phase = Phase::AfterDestination;
        self.consume_after_destination(index, delimiter);
    }

    fn consume_after_destination(&mut self, index: usize, byte: u8) {
        if self.after_destination_cr {
            self.after_destination_cr = false;
            if byte == b'\n' {
                self.fallback_source_end = Some(self.cursor);
                return;
            }
            self.fallback_source_end = Some(index);
            self.consume_after_destination(index, byte);
            return;
        }

        if is_space_or_tab(byte) {
            self.after_destination_separator = true;
            return;
        }
        if !self.after_destination_newline && matches!(byte, b'\r' | b'\n') {
            self.after_destination_separator = true;
            self.after_destination_newline = true;
            if byte == b'\r' {
                self.after_destination_cr = true;
            } else {
                self.fallback_source_end = Some(self.cursor);
            }
            return;
        }

        if !self.after_destination_separator {
            self.reject();
            return;
        }
        match byte {
            b'"' | b'\'' => {
                self.title_start = Some(index);
                self.title_closer = byte;
                self.phase = Phase::QuotedTitle;
            }
            b'(' => {
                self.title_start = Some(index);
                self.title_closer = b')';
                self.phase = Phase::ParenTitle;
            }
            _ => self.fallback_or_reject_without_title(),
        }
    }

    fn consume_quoted_title(&mut self, index: usize, byte: u8) {
        if self.consume_title_escape(byte) {
            return;
        }
        if byte == self.title_closer {
            self.title_end = Some(index + 1);
            self.phase = Phase::AfterTitle;
        }
    }

    fn consume_paren_title(&mut self, index: usize, byte: u8) {
        if self.consume_title_escape(byte) {
            return;
        }
        match byte {
            b')' => {
                self.title_end = Some(index + 1);
                self.phase = Phase::AfterTitle;
            }
            b'(' => self.fallback_or_reject_without_title(),
            _ => {}
        }
    }

    /// Returns true when this byte is the punctuation half of `escaped_char`.
    fn consume_title_escape(&mut self, byte: u8) -> bool {
        if self.title_backslash {
            self.title_backslash = false;
            if is_ascii_punctuation(byte) {
                return true;
            }
        }
        if byte == b'\\' {
            self.title_backslash = true;
        }
        false
    }

    fn consume_after_title(&mut self, _index: usize, byte: u8) {
        if is_space_or_tab(byte) {
            return;
        }
        match byte {
            b'\n' => {
                let end = self.cursor;
                self.accept_without_input(end);
            }
            b'\r' => self.phase = Phase::AfterTitleCr,
            // The donor retains the successfully scanned title/range even
            // when trailing junk forces its source cursor back to the
            // destination-only line ending.
            _ => self.fallback_or_reject_preserving_title(),
        }
    }

    fn finish_at_eof(&mut self, input: &[u8]) {
        match self.phase {
            Phase::BareDestination if self.destination_bytes > 0 && self.bare_depth == 0 => {
                self.destination_end = self.cursor;
                self.accept(input, self.cursor);
            }
            Phase::AfterDestination | Phase::AfterTitle | Phase::AfterTitleCr => {
                self.accept(input, self.cursor);
            }
            Phase::QuotedTitle | Phase::ParenTitle => {
                self.fallback_or_reject_without_title_with_input(input);
            }
            _ => self.reject(),
        }
    }

    fn fallback_or_reject_without_title(&mut self) {
        if let Some(end) = self.fallback_source_end {
            self.title_start = None;
            self.title_end = None;
            self.accept_without_input(end);
        } else {
            self.reject();
        }
    }

    fn fallback_or_reject_without_title_with_input(&mut self, input: &[u8]) {
        if let Some(end) = self.fallback_source_end {
            self.title_start = None;
            self.title_end = None;
            self.accept(input, end);
        } else {
            self.reject();
        }
    }

    fn fallback_or_reject_preserving_title(&mut self) {
        if let Some(end) = self.fallback_source_end {
            self.accept_without_input(end);
        } else {
            self.reject();
        }
    }

    fn accept_without_input(&mut self, source_end: usize) {
        let title = self
            .title_start
            .zip(self.title_end)
            .map(|(start, end)| start..end);
        self.result = Some(ReferenceDefinitionShape {
            source: 0..source_end,
            label: self.label_start..self.label_end,
            destination: self.destination_start..self.destination_end,
            title,
        });
        self.phase = Phase::Done;
        self.done = true;
    }

    fn accept(&mut self, input: &[u8], source_end: usize) {
        debug_assert!(source_end <= input.len());
        self.accept_without_input(source_end);
    }

    fn reject(&mut self) {
        self.result = None;
        self.phase = Phase::Done;
        self.done = true;
    }
}

impl Default for ReferencePrefixJob {
    fn default() -> Self {
        Self::new()
    }
}

fn is_space_or_tab(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t')
}

fn is_url_space(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\r' | b'\n')
}

fn is_ascii_punctuation(byte: u8) -> bool {
    matches!(byte, b'!'..=b'/' | b':'..=b'@' | b'['..=b'`' | b'{'..=b'~')
}

fn is_cmark_space(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\r' | b'\n')
}

fn trim_cmark(input: &[u8], mut range: Range<usize>) -> Range<usize> {
    while range.start < range.end && is_cmark_space(input[range.start]) {
        range.start += 1;
    }
    while range.end > range.start && is_cmark_space(input[range.end - 1]) {
        range.end -= 1;
    }
    range
}
