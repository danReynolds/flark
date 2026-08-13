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

/// Parser-authoritative presentation effect of an applied E1 command.
/// Hosts may use this to retain unaffected projected source while the result
/// revision is being certified; they never infer Markdown structure from the
/// committed replacement bytes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DocumentEditPresentationTransitionV1 {
    None,
    SplitParagraph,
    ContinueList,
    ExitList,
    MergeParagraph,
    LiftList,
    ContinueBlockQuote,
    ExitBlockQuote,
    LiftBlockQuote,
    ExitHeading,
    LiftHeading,
    OutdentList,
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
    /// Exact post-splice byte caret used by the ABI to verify transformed
    /// canonical anchors without a second actor coordinate query.
    pub result_selection_byte: usize,
    pub result_selection_utf16: usize,
    pub result_source_byte_length: usize,
    pub result_source_utf16_length: usize,
    pub parser_pending: bool,
    pub presentation_transition: DocumentEditPresentationTransitionV1,
}

/// Result of one literal source transaction. The caller already knows the
/// replacement bytes; the runtime still owns coordinate validation, inverse
/// capture, and the single source commit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DocumentSourceTransactionReceiptV1 {
    pub base_revision: u64,
    pub result_revision: u64,
    pub committed_splice: DocumentCommittedSpliceV1,
    pub inverse: Vec<u8>,
    pub result_selection_base_utf16: usize,
    pub result_selection_extent_utf16: usize,
    pub result_selection_base_byte: usize,
    pub result_selection_extent_byte: usize,
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
        nesting_depth: u8,
        marker_offset: u8,
        starts_list: bool,
        task_checked: Option<bool>,
        empty: bool,
        outdent: Option<DocumentListOutdent>,
    },
    AtxHeading {
        prefix_bytes: Range<usize>,
        prefix_utf16: Range<usize>,
        empty: bool,
    },
    BlockQuote {
        prefix_bytes: Range<usize>,
        prefix_utf16: Range<usize>,
        prefix_text: String,
        starts_quote: bool,
        empty: bool,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DocumentListOutdent {
    pub(crate) bytes: Range<usize>,
    pub(crate) utf16: Range<usize>,
    pub(crate) indentation: String,
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
    pub(crate) presentation_transition: DocumentEditPresentationTransitionV1,
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
            applied(
                splice,
                result_selection_utf16,
                Some(result_context),
                DocumentEditPresentationTransitionV1::SplitParagraph,
            )
        }
        (
            DocumentEditIntentV1::InsertParagraphBreak,
            DocumentSimpleEditRow::AtxHeading {
                prefix_bytes,
                prefix_utf16,
                empty,
            },
        ) => {
            if *empty || context.editable_bytes.is_empty() {
                let replacement = if context.source_bytes.end == prefix_bytes.end {
                    context.ending.text().to_owned()
                } else {
                    String::new()
                };
                return clear_prefixed_row(
                    context,
                    prefix_bytes.clone(),
                    prefix_utf16.clone(),
                    replacement,
                    DocumentEditPresentationTransitionV1::ExitHeading,
                );
            }
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
            applied(
                splice,
                result_selection_utf16,
                Some(result_context),
                DocumentEditPresentationTransitionV1::SplitParagraph,
            )
        }
        (
            DocumentEditIntentV1::InsertParagraphBreak,
            DocumentSimpleEditRow::BlockQuote {
                prefix_bytes,
                prefix_utf16,
                prefix_text,
                starts_quote,
                empty,
            },
        ) => {
            if *empty || context.editable_bytes.is_empty() {
                let existing_terminal_ending = prefix_bytes
                    .end
                    .checked_add(context.ending.text().len())
                    .is_some_and(|ending_end| context.source_bytes.start == ending_end);
                let replacement = if context.source_bytes.end == prefix_bytes.end {
                    context.ending.text().to_owned()
                } else if existing_terminal_ending {
                    String::new()
                } else if !starts_quote {
                    context.ending.text().to_owned()
                } else {
                    String::new()
                };
                return clear_prefixed_row(
                    context,
                    prefix_bytes.clone(),
                    prefix_utf16.clone(),
                    replacement,
                    DocumentEditPresentationTransitionV1::ExitBlockQuote,
                );
            }
            let replacement = format!("{}{prefix_text}", context.ending.text());
            let result_selection_utf16 = selection_utf16 + replacement.encode_utf16().count();
            let splice = splice(
                selection_byte..selection_byte,
                selection_utf16..selection_utf16,
                replacement,
            );
            let byte_delta = splice.replacement.len();
            let utf16_delta = splice.replacement.encode_utf16().count();
            let ending_bytes = context.ending.text().len();
            let ending_utf16 = context.ending.text().encode_utf16().count();
            let prefix_start_byte = selection_byte + ending_bytes;
            let prefix_start_utf16 = selection_utf16 + ending_utf16;
            let result_context = DocumentSimpleEditContext {
                revision: context.revision + 1,
                source_bytes: selection_byte + byte_delta..context.source_bytes.end + byte_delta,
                source_utf16: selection_utf16 + utf16_delta..context.source_utf16.end + utf16_delta,
                editable_bytes: selection_byte + byte_delta
                    ..context.editable_bytes.end + byte_delta,
                editable_utf16: selection_utf16 + utf16_delta
                    ..context.editable_utf16.end + utf16_delta,
                ending: context.ending,
                row: DocumentSimpleEditRow::BlockQuote {
                    prefix_bytes: prefix_start_byte..selection_byte + byte_delta,
                    prefix_utf16: prefix_start_utf16..selection_utf16 + utf16_delta,
                    prefix_text: prefix_text.clone(),
                    starts_quote: false,
                    empty: selection_byte == context.editable_bytes.end,
                },
                paragraph_merge: None,
            };
            applied(
                splice,
                result_selection_utf16,
                Some(result_context),
                DocumentEditPresentationTransitionV1::ContinueBlockQuote,
            )
        }
        (
            DocumentEditIntentV1::InsertParagraphBreak,
            DocumentSimpleEditRow::ListItem {
                marker,
                prefix_bytes,
                prefix_utf16,
                nesting_depth,
                marker_offset,
                starts_list,
                task_checked,
                empty,
                outdent,
            },
        ) => {
            if *empty || context.editable_bytes.is_empty() {
                if *nesting_depth == 2 {
                    return outdent.as_ref().map_or_else(
                        || {
                            disposition(
                                DocumentEditIntentDispositionV1::NeedsCurrentSemantics,
                                selection_utf16,
                            )
                        },
                        |outdent| {
                            outdent_list_row(
                                context,
                                *marker,
                                prefix_bytes,
                                prefix_utf16,
                                *marker_offset,
                                *task_checked,
                                true,
                                outdent,
                            )
                        },
                    );
                }
                // Removing an empty marker must still leave a physical plain
                // row. A continuing list row also needs separation from the
                // preceding list; otherwise the next typed paragraph becomes
                // a CommonMark lazy continuation of the prior item. Its owned
                // line ending remains the new row's terminal ending. A list
                // that starts on this row already has whatever separation its
                // preceding context requires. An unterminated EOF row needs
                // one physical ending in either case.
                let replacement = if context.source_bytes.end == prefix_bytes.end || !starts_list {
                    context.ending.text().to_owned()
                } else {
                    String::new()
                };
                return clear_prefixed_row(
                    context,
                    prefix_bytes.clone(),
                    prefix_utf16.clone(),
                    replacement,
                    DocumentEditPresentationTransitionV1::ExitList,
                );
            }

            let marker_text = next_marker_text(*marker);
            let task_prefix = task_checked.map_or("", |_| "[ ] ");
            let container_indent = outdent
                .as_ref()
                .map_or("", |outdent| outdent.indentation.as_str());
            let prefix = format!(
                "{container_indent}{}{marker_text} {task_prefix}",
                " ".repeat(usize::from(*marker_offset))
            );
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
            let indentation_bytes = container_indent.len();
            let indentation_utf16 = container_indent.encode_utf16().count();
            let prefix_start_byte = selection_byte + line_ending_bytes + indentation_bytes;
            let prefix_start_utf16 = selection_utf16 + line_ending_utf16 + indentation_utf16;
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
                    nesting_depth: *nesting_depth,
                    marker_offset: *marker_offset,
                    starts_list: false,
                    task_checked: task_checked.map(|_| false),
                    empty: selection_byte == context.editable_bytes.end,
                    outdent: outdent.as_ref().map(|_| DocumentListOutdent {
                        bytes: selection_byte + line_ending_bytes..prefix_start_byte,
                        utf16: selection_utf16 + line_ending_utf16..prefix_start_utf16,
                        indentation: container_indent.to_owned(),
                    }),
                },
                paragraph_merge: None,
            };
            applied(
                splice,
                result_selection_utf16,
                Some(result_context),
                DocumentEditPresentationTransitionV1::ContinueList,
            )
        }
        (
            DocumentEditIntentV1::DeleteBackward,
            DocumentSimpleEditRow::AtxHeading {
                prefix_bytes,
                prefix_utf16,
                ..
            },
        ) if selection_byte == context.editable_bytes.start
            && selection_utf16 == context.editable_utf16.start =>
        {
            clear_prefixed_row(
                context,
                prefix_bytes.clone(),
                prefix_utf16.clone(),
                String::new(),
                DocumentEditPresentationTransitionV1::LiftHeading,
            )
        }
        (
            DocumentEditIntentV1::DeleteBackward,
            DocumentSimpleEditRow::BlockQuote {
                prefix_bytes,
                prefix_utf16,
                starts_quote,
                ..
            },
        ) if selection_byte == context.editable_bytes.start
            && selection_utf16 == context.editable_utf16.start =>
        {
            clear_prefixed_row(
                context,
                prefix_bytes.clone(),
                prefix_utf16.clone(),
                if *starts_quote {
                    String::new()
                } else {
                    context.ending.text().to_owned()
                },
                DocumentEditPresentationTransitionV1::LiftBlockQuote,
            )
        }
        (
            DocumentEditIntentV1::DeleteBackward,
            DocumentSimpleEditRow::ListItem {
                marker,
                prefix_bytes,
                prefix_utf16,
                nesting_depth,
                marker_offset,
                starts_list,
                task_checked,
                empty,
                outdent,
                ..
            },
        ) if selection_byte == context.editable_bytes.start
            && selection_utf16 == context.editable_utf16.start =>
        {
            if *nesting_depth == 2 {
                return outdent.as_ref().map_or_else(
                    || {
                        disposition(
                            DocumentEditIntentDispositionV1::NeedsCurrentSemantics,
                            selection_utf16,
                        )
                    },
                    |outdent| {
                        outdent_list_row(
                            context,
                            *marker,
                            prefix_bytes,
                            prefix_utf16,
                            *marker_offset,
                            *task_checked,
                            *empty,
                            outdent,
                        )
                    },
                );
            }
            let replacement = if *starts_list {
                String::new()
            } else {
                context.ending.text().to_owned()
            };
            clear_prefixed_row(
                context,
                prefix_bytes.clone(),
                prefix_utf16.clone(),
                replacement,
                DocumentEditPresentationTransitionV1::LiftList,
            )
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
            applied(
                splice,
                result_selection_utf16,
                Some(result_context),
                DocumentEditPresentationTransitionV1::MergeParagraph,
            )
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
    presentation_transition: DocumentEditPresentationTransitionV1,
) -> ResolvedDocumentEditIntentV1 {
    ResolvedDocumentEditIntentV1 {
        disposition: DocumentEditIntentDispositionV1::Applied,
        splice: Some(splice),
        result_selection_utf16,
        result_context,
        presentation_transition,
    }
}

fn clear_prefixed_row(
    context: &DocumentSimpleEditContext,
    prefix_bytes: Range<usize>,
    prefix_utf16: Range<usize>,
    replacement: String,
    transition: DocumentEditPresentationTransitionV1,
) -> ResolvedDocumentEditIntentV1 {
    let splice = splice(prefix_bytes.clone(), prefix_utf16.clone(), replacement);
    let replacement_bytes = splice.replacement.len();
    let replacement_utf16 = splice.replacement.encode_utf16().count();
    let byte_delta = replacement_bytes as isize - prefix_bytes.len() as isize;
    let utf16_delta = replacement_utf16 as isize - prefix_utf16.len() as isize;
    let content_start_byte = add_signed(prefix_bytes.start, replacement_bytes as isize);
    let content_start_utf16 = add_signed(prefix_utf16.start, replacement_utf16 as isize);
    let result_context = DocumentSimpleEditContext {
        revision: context.revision + 1,
        source_bytes: content_start_byte..add_signed(context.source_bytes.end, byte_delta),
        source_utf16: content_start_utf16..add_signed(context.source_utf16.end, utf16_delta),
        editable_bytes: content_start_byte..add_signed(context.editable_bytes.end, byte_delta),
        editable_utf16: content_start_utf16..add_signed(context.editable_utf16.end, utf16_delta),
        ending: context.ending,
        row: DocumentSimpleEditRow::Plain,
        paragraph_merge: None,
    };
    applied(
        splice,
        content_start_utf16,
        Some(result_context),
        transition,
    )
}

#[allow(clippy::too_many_arguments)]
fn outdent_list_row(
    context: &DocumentSimpleEditContext,
    marker: DocumentListMarker,
    prefix_bytes: &Range<usize>,
    prefix_utf16: &Range<usize>,
    marker_offset: u8,
    task_checked: Option<bool>,
    empty: bool,
    outdent: &DocumentListOutdent,
) -> ResolvedDocumentEditIntentV1 {
    if outdent.bytes.end != prefix_bytes.start
        || outdent.utf16.end != prefix_utf16.start
        || outdent.bytes.is_empty()
        || outdent.utf16.is_empty()
    {
        return disposition(
            DocumentEditIntentDispositionV1::NeedsCurrentSemantics,
            context.editable_utf16.start,
        );
    }
    let removed_bytes = outdent.bytes.len();
    let removed_utf16 = outdent.utf16.len();
    let shifted = (
        prefix_bytes.end.checked_sub(removed_bytes),
        prefix_utf16.end.checked_sub(removed_utf16),
        context.source_bytes.start.checked_sub(removed_bytes),
        context.source_bytes.end.checked_sub(removed_bytes),
        context.source_utf16.start.checked_sub(removed_utf16),
        context.source_utf16.end.checked_sub(removed_utf16),
        context.editable_bytes.start.checked_sub(removed_bytes),
        context.editable_bytes.end.checked_sub(removed_bytes),
        context.editable_utf16.start.checked_sub(removed_utf16),
        context.editable_utf16.end.checked_sub(removed_utf16),
    );
    let (
        Some(prefix_end_byte),
        Some(prefix_end_utf16),
        Some(source_start_byte),
        Some(source_end_byte),
        Some(source_start_utf16),
        Some(source_end_utf16),
        Some(editable_start_byte),
        Some(editable_end_byte),
        Some(editable_start_utf16),
        Some(editable_end_utf16),
    ) = shifted
    else {
        return disposition(
            DocumentEditIntentDispositionV1::NeedsCurrentSemantics,
            context.editable_utf16.start,
        );
    };
    let splice = splice(outdent.bytes.clone(), outdent.utf16.clone(), String::new());
    let result_prefix_bytes = outdent.bytes.start..prefix_end_byte;
    let result_prefix_utf16 = outdent.utf16.start..prefix_end_utf16;
    let result_context = DocumentSimpleEditContext {
        revision: context.revision + 1,
        source_bytes: source_start_byte..source_end_byte,
        source_utf16: source_start_utf16..source_end_utf16,
        editable_bytes: editable_start_byte..editable_end_byte,
        editable_utf16: editable_start_utf16..editable_end_utf16,
        ending: context.ending,
        row: DocumentSimpleEditRow::ListItem {
            marker,
            prefix_bytes: result_prefix_bytes,
            prefix_utf16: result_prefix_utf16,
            nesting_depth: 1,
            marker_offset,
            starts_list: false,
            task_checked,
            empty,
            outdent: None,
        },
        paragraph_merge: None,
    };
    applied(
        splice,
        result_context.editable_utf16.start,
        Some(result_context),
        DocumentEditPresentationTransitionV1::OutdentList,
    )
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
        presentation_transition: DocumentEditPresentationTransitionV1::None,
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
