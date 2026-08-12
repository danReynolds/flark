use std::ops::Range;

use crate::{DocumentListDelimiter, DocumentListMarker};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DocumentEditIntentV1 {
    InsertParagraphBreak,
    DeleteBackward,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DocumentEditIntentDispositionV1 {
    Applied,
    HandledNoChange,
    NotApplicable,
    NeedsCurrentSemantics,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DocumentCommittedSpliceV1 {
    pub base_byte_range: Range<usize>,
    pub base_utf16_range: Range<usize>,
    pub replacement: String,
    pub result_byte_range: Range<usize>,
    pub result_utf16_range: Range<usize>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DocumentEditIntentReceiptV1 {
    pub disposition: DocumentEditIntentDispositionV1,
    pub base_revision: u64,
    pub result_revision: u64,
    pub committed_splice: Option<DocumentCommittedSpliceV1>,
    /// Exact deleted bytes captured before the source linearization point.
    /// Hosts use this to retain one required inverse without a second actor
    /// round trip. It is intentionally not part of the public C receipt.
    pub inverse: Vec<u8>,
    pub result_selection_utf16: usize,
    pub result_source_byte_length: usize,
    pub result_source_utf16_length: usize,
    pub parser_pending: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DocumentEditLineEnding {
    Lf,
    CrLf,
    Cr,
}

impl DocumentEditLineEnding {
    pub(crate) const fn text(self) -> &'static str {
        match self {
            Self::Lf => "\n",
            Self::CrLf => "\r\n",
            Self::Cr => "\r",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum DocumentSimpleEditRow {
    Plain,
    ListItem {
        marker: DocumentListMarker,
        prefix_bytes: Range<usize>,
        prefix_utf16: Range<usize>,
        marker_offset: u8,
        starts_list: bool,
        empty: bool,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DocumentParagraphMerge {
    pub(crate) previous_source_bytes: Range<usize>,
    pub(crate) previous_source_utf16: Range<usize>,
    pub(crate) separator_bytes: Range<usize>,
    pub(crate) separator_utf16: Range<usize>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DocumentSimpleEditContext {
    pub(crate) revision: u64,
    pub(crate) source_bytes: Range<usize>,
    pub(crate) source_utf16: Range<usize>,
    pub(crate) editable_bytes: Range<usize>,
    pub(crate) editable_utf16: Range<usize>,
    pub(crate) ending: DocumentEditLineEnding,
    pub(crate) row: DocumentSimpleEditRow,
    pub(crate) paragraph_merge: Option<DocumentParagraphMerge>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ResolvedDocumentEditIntentV1 {
    pub(crate) disposition: DocumentEditIntentDispositionV1,
    pub(crate) splice: Option<DocumentCommittedSpliceV1>,
    pub(crate) result_selection_utf16: usize,
    pub(crate) result_context: Option<DocumentSimpleEditContext>,
}

pub(crate) fn resolve_document_edit_intent_v1(
    intent: DocumentEditIntentV1,
    selection_byte: usize,
    selection_utf16: usize,
    context: &DocumentSimpleEditContext,
) -> ResolvedDocumentEditIntentV1 {
    if context.revision == 0
        || selection_byte < context.editable_bytes.start
        || selection_byte > context.editable_bytes.end
        || selection_utf16 < context.editable_utf16.start
        || selection_utf16 > context.editable_utf16.end
    {
        return disposition(
            DocumentEditIntentDispositionV1::NeedsCurrentSemantics,
            selection_utf16,
        );
    }

    match (intent, &context.row) {
        (DocumentEditIntentV1::InsertParagraphBreak, DocumentSimpleEditRow::Plain) => {
            let ending = context.ending.text();
            let replacement = format!("{ending}{ending}");
            let result_selection_utf16 = selection_utf16 + replacement.encode_utf16().count();
            let splice = splice(
                selection_byte..selection_byte,
                selection_utf16..selection_utf16,
                replacement,
            );
            let byte_delta = splice.replacement.len();
            let utf16_delta = splice.replacement.encode_utf16().count();
            let result_context = DocumentSimpleEditContext {
                revision: context.revision + 1,
                source_bytes: selection_byte + byte_delta..context.source_bytes.end + byte_delta,
                source_utf16: selection_utf16 + utf16_delta..context.source_utf16.end + utf16_delta,
                editable_bytes: selection_byte + byte_delta
                    ..context.editable_bytes.end + byte_delta,
                editable_utf16: selection_utf16 + utf16_delta
                    ..context.editable_utf16.end + utf16_delta,
                ending: context.ending,
                row: DocumentSimpleEditRow::Plain,
                paragraph_merge: None,
            };
            applied(splice, result_selection_utf16, Some(result_context))
        }
        (
            DocumentEditIntentV1::InsertParagraphBreak,
            DocumentSimpleEditRow::ListItem {
                marker,
                prefix_bytes,
                prefix_utf16,
                marker_offset,
                starts_list: _,
                empty,
            },
        ) => {
            if *empty || context.editable_bytes.is_empty() {
                // Removing an empty marker must still leave a physical blank
                // line. If the row already owns a line ending, retaining that
                // ending is sufficient; an unterminated EOF row needs one in
                // the replacement.
                let replacement = if context.source_bytes.end == prefix_bytes.end {
                    context.ending.text().to_owned()
                } else {
                    String::new()
                };
                let splice = splice(prefix_bytes.clone(), prefix_utf16.clone(), replacement);
                let result_selection_utf16 =
                    prefix_utf16.start + splice.replacement.encode_utf16().count();
                let removed_bytes = prefix_bytes.end - prefix_bytes.start;
                let removed_utf16 = prefix_utf16.end - prefix_utf16.start;
                let replacement_bytes = splice.replacement.len();
                let replacement_utf16 = splice.replacement.encode_utf16().count();
                let result_context = DocumentSimpleEditContext {
                    revision: context.revision + 1,
                    source_bytes: prefix_bytes.start
                        ..context
                            .source_bytes
                            .end
                            .saturating_sub(removed_bytes)
                            .saturating_add(replacement_bytes),
                    source_utf16: prefix_utf16.start
                        ..context
                            .source_utf16
                            .end
                            .saturating_sub(removed_utf16)
                            .saturating_add(replacement_utf16),
                    editable_bytes: prefix_bytes.start + replacement_bytes
                        ..prefix_bytes.start + replacement_bytes,
                    editable_utf16: prefix_utf16.start + replacement_utf16
                        ..prefix_utf16.start + replacement_utf16,
                    ending: context.ending,
                    row: DocumentSimpleEditRow::Plain,
                    paragraph_merge: None,
                };
                return applied(splice, result_selection_utf16, Some(result_context));
            }

            let marker_text = next_marker_text(*marker);
            let prefix = format!("{}{marker_text} ", " ".repeat(usize::from(*marker_offset)));
            let replacement = format!("{}{prefix}", context.ending.text());
            let result_selection_utf16 = selection_utf16 + replacement.encode_utf16().count();
            let splice = splice(
                selection_byte..selection_byte,
                selection_utf16..selection_utf16,
                replacement,
            );
            let byte_delta = splice.replacement.len();
            let utf16_delta = splice.replacement.encode_utf16().count();
            let line_ending_bytes = context.ending.text().len();
            let line_ending_utf16 = context.ending.text().encode_utf16().count();
            let prefix_start_byte = selection_byte + line_ending_bytes;
            let prefix_start_utf16 = selection_utf16 + line_ending_utf16;
            let result_context = DocumentSimpleEditContext {
                revision: context.revision + 1,
                source_bytes: selection_byte + byte_delta..context.source_bytes.end + byte_delta,
                source_utf16: selection_utf16 + utf16_delta..context.source_utf16.end + utf16_delta,
                editable_bytes: selection_byte + byte_delta
                    ..context.editable_bytes.end + byte_delta,
                editable_utf16: selection_utf16 + utf16_delta
                    ..context.editable_utf16.end + utf16_delta,
                ending: context.ending,
                row: DocumentSimpleEditRow::ListItem {
                    marker: next_marker(*marker),
                    prefix_bytes: prefix_start_byte..selection_byte + byte_delta,
                    prefix_utf16: prefix_start_utf16..selection_utf16 + utf16_delta,
                    marker_offset: *marker_offset,
                    starts_list: false,
                    empty: selection_byte == context.editable_bytes.end,
                },
                paragraph_merge: None,
            };
            applied(splice, result_selection_utf16, Some(result_context))
        }
        (
            DocumentEditIntentV1::DeleteBackward,
            DocumentSimpleEditRow::ListItem {
                prefix_bytes,
                prefix_utf16,
                starts_list,
                ..
            },
        ) if selection_byte == context.editable_bytes.start
            && selection_utf16 == context.editable_utf16.start =>
        {
            let replacement = if *starts_list {
                String::new()
            } else {
                context.ending.text().to_owned()
            };
            let splice = splice(prefix_bytes.clone(), prefix_utf16.clone(), replacement);
            let result_selection_utf16 =
                prefix_utf16.start + splice.replacement.encode_utf16().count();
            let byte_delta = splice.replacement.len() as isize
                - (prefix_bytes.end - prefix_bytes.start) as isize;
            let utf16_delta = splice.replacement.encode_utf16().count() as isize
                - (prefix_utf16.end - prefix_utf16.start) as isize;
            let content_start_byte =
                add_signed(prefix_bytes.start, splice.replacement.len() as isize);
            let content_start_utf16 = add_signed(
                prefix_utf16.start,
                splice.replacement.encode_utf16().count() as isize,
            );
            let result_context = DocumentSimpleEditContext {
                revision: context.revision + 1,
                source_bytes: content_start_byte..add_signed(context.source_bytes.end, byte_delta),
                source_utf16: content_start_utf16
                    ..add_signed(context.source_utf16.end, utf16_delta),
                editable_bytes: content_start_byte
                    ..add_signed(context.editable_bytes.end, byte_delta),
                editable_utf16: content_start_utf16
                    ..add_signed(context.editable_utf16.end, utf16_delta),
                ending: context.ending,
                row: DocumentSimpleEditRow::Plain,
                paragraph_merge: None,
            };
            applied(splice, result_selection_utf16, Some(result_context))
        }
        (DocumentEditIntentV1::DeleteBackward, DocumentSimpleEditRow::Plain)
            if selection_byte == context.editable_bytes.start
                && selection_utf16 == context.editable_utf16.start =>
        {
            let Some(merge) = &context.paragraph_merge else {
                return disposition(
                    if selection_byte == 0 {
                        DocumentEditIntentDispositionV1::HandledNoChange
                    } else {
                        DocumentEditIntentDispositionV1::NotApplicable
                    },
                    selection_utf16,
                );
            };
            let splice = splice(
                merge.separator_bytes.clone(),
                merge.separator_utf16.clone(),
                String::new(),
            );
            let result_selection_utf16 = merge.separator_utf16.start;
            let removed_bytes = merge.separator_bytes.end - merge.separator_bytes.start;
            let removed_utf16 = merge.separator_utf16.end - merge.separator_utf16.start;
            let result_context = DocumentSimpleEditContext {
                revision: context.revision + 1,
                source_bytes: merge.previous_source_bytes.start
                    ..context.source_bytes.end - removed_bytes,
                source_utf16: merge.previous_source_utf16.start
                    ..context.source_utf16.end - removed_utf16,
                editable_bytes: merge.previous_source_bytes.start
                    ..context.editable_bytes.end - removed_bytes,
                editable_utf16: merge.previous_source_utf16.start
                    ..context.editable_utf16.end - removed_utf16,
                ending: context.ending,
                row: DocumentSimpleEditRow::Plain,
                paragraph_merge: None,
            };
            applied(splice, result_selection_utf16, Some(result_context))
        }
        _ => disposition(
            DocumentEditIntentDispositionV1::NotApplicable,
            selection_utf16,
        ),
    }
}

fn splice(
    base_byte_range: Range<usize>,
    base_utf16_range: Range<usize>,
    replacement: String,
) -> DocumentCommittedSpliceV1 {
    let result_byte_end = base_byte_range.start + replacement.len();
    let result_utf16_end = base_utf16_range.start + replacement.encode_utf16().count();
    DocumentCommittedSpliceV1 {
        result_byte_range: base_byte_range.start..result_byte_end,
        result_utf16_range: base_utf16_range.start..result_utf16_end,
        base_byte_range,
        base_utf16_range,
        replacement,
    }
}

fn applied(
    splice: DocumentCommittedSpliceV1,
    result_selection_utf16: usize,
    result_context: Option<DocumentSimpleEditContext>,
) -> ResolvedDocumentEditIntentV1 {
    ResolvedDocumentEditIntentV1 {
        disposition: DocumentEditIntentDispositionV1::Applied,
        splice: Some(splice),
        result_selection_utf16,
        result_context,
    }
}

fn disposition(
    disposition: DocumentEditIntentDispositionV1,
    result_selection_utf16: usize,
) -> ResolvedDocumentEditIntentV1 {
    ResolvedDocumentEditIntentV1 {
        disposition,
        splice: None,
        result_selection_utf16,
        result_context: None,
    }
}

fn next_marker(marker: DocumentListMarker) -> DocumentListMarker {
    match marker {
        DocumentListMarker::Ordered { value, delimiter } => DocumentListMarker::Ordered {
            value: value.saturating_add(1).min(999_999_999),
            delimiter,
        },
        bullet => bullet,
    }
}

fn next_marker_text(marker: DocumentListMarker) -> String {
    match next_marker(marker) {
        DocumentListMarker::Bullet(crate::DocumentBulletMarker::Hyphen) => "-".to_owned(),
        DocumentListMarker::Bullet(crate::DocumentBulletMarker::Plus) => "+".to_owned(),
        DocumentListMarker::Bullet(crate::DocumentBulletMarker::Asterisk) => "*".to_owned(),
        DocumentListMarker::Ordered { value, delimiter } => match delimiter {
            DocumentListDelimiter::Period => format!("{value}."),
            DocumentListDelimiter::Parenthesis => format!("{value})"),
        },
    }
}

fn add_signed(value: usize, delta: isize) -> usize {
    if delta >= 0 {
        value + delta as usize
    } else {
        value - delta.unsigned_abs()
    }
}
