//! Resumable correspondent of Comrak scanner `html_block_start_7`.

use serde::{Deserialize, Serialize};

use crate::{CancellationToken, Poll, ScanReceipt, physical_content_end};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum Phase {
    LessThan,
    AfterLessThan,
    OpenTagName,
    CloseTagNameStart,
    CloseTagName,
    OpenSpace,
    CloseSpace,
    AttributeName,
    AfterAttributeSpace,
    BeforeValue,
    UnquotedValue,
    SingleQuotedValue,
    DoubleQuotedValue,
    AfterValue,
    ExpectGreaterThan,
    Trailing,
    Done,
}

/// Exact regular-language state for CommonMark HTML block start type 7.
///
/// Input starts at first nonspace. Types 1--6 must be tested first because the
/// donor returns those earlier even when the same line is also a valid type-7
/// tag.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HtmlType7Job {
    cursor: usize,
    end: usize,
    phase: Phase,
    closing: bool,
    done: bool,
    result: bool,
    receipt: ScanReceipt,
}

impl HtmlType7Job {
    pub fn new(input: &[u8]) -> Self {
        Self {
            cursor: 0,
            end: physical_content_end(input),
            phase: Phase::LessThan,
            closing: false,
            done: false,
            result: false,
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
    ) -> Poll<bool> {
        assert!(fuel > 0);
        assert_eq!(self.end, physical_content_end(input));
        if cancellation.is_cancelled() {
            self.receipt.record_poll(0);
            return Poll::Cancelled { inspected: 0 };
        }
        let mut inspected = 0;
        while !self.done && inspected < fuel {
            if self.cursor == self.end {
                self.finish_at_end();
                break;
            }
            let byte = input[self.cursor];
            self.cursor += 1;
            inspected += 1;
            self.consume(byte);
        }
        self.receipt.record_poll(inspected);
        if cancellation.is_cancelled() {
            Poll::Cancelled { inspected }
        } else if self.done {
            Poll::Ready {
                value: self.result,
                inspected,
            }
        } else {
            Poll::Pending { inspected }
        }
    }

    fn consume(&mut self, byte: u8) {
        match self.phase {
            Phase::LessThan => {
                if byte == b'<' {
                    self.phase = Phase::AfterLessThan;
                } else {
                    self.reject();
                }
            }
            Phase::AfterLessThan => {
                if byte == b'/' {
                    self.closing = true;
                    self.phase = Phase::CloseTagNameStart;
                } else if byte.is_ascii_alphabetic() {
                    self.phase = Phase::OpenTagName;
                } else {
                    self.reject();
                }
            }
            Phase::OpenTagName => {
                if is_tag_name_continue(byte) {
                    return;
                }
                self.consume_after_open_tag_name(byte);
            }
            Phase::CloseTagNameStart => {
                if byte.is_ascii_alphabetic() {
                    self.phase = Phase::CloseTagName;
                } else {
                    self.reject();
                }
            }
            Phase::CloseTagName => {
                if is_tag_name_continue(byte) {
                    return;
                }
                if is_html_space(byte) {
                    self.phase = Phase::CloseSpace;
                } else if byte == b'>' {
                    self.phase = Phase::Trailing;
                } else {
                    self.reject();
                }
            }
            Phase::OpenSpace => {
                if is_html_space(byte) {
                    return;
                }
                if byte == b'>' {
                    self.phase = Phase::Trailing;
                } else if byte == b'/' {
                    self.phase = Phase::ExpectGreaterThan;
                } else if is_attribute_name_start(byte) {
                    self.phase = Phase::AttributeName;
                } else {
                    self.reject();
                }
            }
            Phase::CloseSpace => {
                if is_html_space(byte) {
                    return;
                }
                if byte == b'>' {
                    self.phase = Phase::Trailing;
                } else {
                    self.reject();
                }
            }
            Phase::AttributeName => {
                if is_attribute_name_continue(byte) {
                    return;
                }
                if byte == b'=' {
                    self.phase = Phase::BeforeValue;
                } else if is_html_space(byte) {
                    self.phase = Phase::AfterAttributeSpace;
                } else if byte == b'>' {
                    self.phase = Phase::Trailing;
                } else if byte == b'/' {
                    self.phase = Phase::ExpectGreaterThan;
                } else {
                    self.reject();
                }
            }
            Phase::AfterAttributeSpace => {
                if is_html_space(byte) {
                    return;
                }
                if byte == b'=' {
                    self.phase = Phase::BeforeValue;
                } else if byte == b'>' {
                    self.phase = Phase::Trailing;
                } else if byte == b'/' {
                    self.phase = Phase::ExpectGreaterThan;
                } else if is_attribute_name_start(byte) {
                    self.phase = Phase::AttributeName;
                } else {
                    self.reject();
                }
            }
            Phase::BeforeValue => {
                if is_html_space(byte) {
                    return;
                }
                self.phase = match byte {
                    b'\'' => Phase::SingleQuotedValue,
                    b'"' => Phase::DoubleQuotedValue,
                    _ if is_unquoted_value(byte) => Phase::UnquotedValue,
                    _ => {
                        self.reject();
                        return;
                    }
                };
            }
            Phase::UnquotedValue => {
                if is_unquoted_value(byte) {
                    return;
                }
                if is_html_space(byte) {
                    self.phase = Phase::OpenSpace;
                } else if byte == b'>' {
                    self.phase = Phase::Trailing;
                } else {
                    self.reject();
                }
            }
            Phase::SingleQuotedValue => {
                if byte == b'\'' {
                    self.phase = Phase::AfterValue;
                }
            }
            Phase::DoubleQuotedValue => {
                if byte == b'"' {
                    self.phase = Phase::AfterValue;
                }
            }
            Phase::AfterValue => {
                if is_html_space(byte) {
                    self.phase = Phase::OpenSpace;
                } else if byte == b'>' {
                    self.phase = Phase::Trailing;
                } else if byte == b'/' {
                    self.phase = Phase::ExpectGreaterThan;
                } else {
                    self.reject();
                }
            }
            Phase::ExpectGreaterThan => {
                if byte == b'>' {
                    self.phase = Phase::Trailing;
                } else {
                    self.reject();
                }
            }
            Phase::Trailing => {
                if !is_type7_trailing_space(byte) {
                    self.reject();
                }
            }
            Phase::Done => unreachable!("completed state returned before consume"),
        }
    }

    fn consume_after_open_tag_name(&mut self, byte: u8) {
        if is_html_space(byte) {
            self.phase = Phase::OpenSpace;
        } else if byte == b'>' {
            self.phase = Phase::Trailing;
        } else if byte == b'/' {
            self.phase = Phase::ExpectGreaterThan;
        } else {
            self.reject();
        }
    }

    fn finish_at_end(&mut self) {
        self.result = self.phase == Phase::Trailing;
        self.phase = Phase::Done;
        self.done = true;
    }

    fn reject(&mut self) {
        self.phase = Phase::Done;
        self.result = false;
        self.done = true;
    }
}

fn is_tag_name_continue(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'-'
}

fn is_attribute_name_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || matches!(byte, b'_' | b':')
}

fn is_attribute_name_continue(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b':' | b'.' | b'_' | b'-')
}

fn is_html_space(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | 0x0b | 0x0c | b'\r' | b'\n')
}

fn is_type7_trailing_space(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | 0x0b | 0x0c)
}

fn is_unquoted_value(byte: u8) -> bool {
    !is_html_space(byte) && !matches!(byte, b'"' | b'\'' | b'=' | b'<' | b'>' | b'`')
}
