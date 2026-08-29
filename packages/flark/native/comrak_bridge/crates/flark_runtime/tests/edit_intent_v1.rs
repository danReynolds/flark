use std::time::{Duration, Instant};

use flark_runtime::{
    DocumentEditIntentDispositionV1, DocumentEditIntentV1, DocumentEditPresentationTransitionV1,
    DocumentInlineContinuationScalarPolicyV1, DocumentSession, DocumentSessionPhase,
};

fn pump_ready(document: &mut DocumentSession) {
    let mut turns = 0;
    while document.phase() != DocumentSessionPhase::Ready {
        document.pump(512).expect("parser pump");
        turns += 1;
        assert!(turns < 1_000_000, "fixture must converge");
    }
}

fn source(document: &DocumentSession) -> String {
    String::from_utf8(
        document
            .source_bytes(0..document.source_byte_len())
            .expect("source bytes"),
    )
    .expect("UTF-8 source")
}

#[test]
fn deleting_the_final_rendered_inline_grapheme_removes_its_parser_owned_closure() {
    let cases = [
        (
            "emphasis Backspace",
            "A *t* Z",
            4,
            DocumentEditIntentV1::DeleteBackward,
        ),
        (
            "emphasis Delete",
            "A *t* Z",
            3,
            DocumentEditIntentV1::DeleteForward,
        ),
        (
            "strong Backspace",
            "A **t** Z",
            5,
            DocumentEditIntentV1::DeleteBackward,
        ),
        (
            "strong Delete",
            "A **t** Z",
            4,
            DocumentEditIntentV1::DeleteForward,
        ),
        (
            "strike Backspace",
            "A ~~t~~ Z",
            5,
            DocumentEditIntentV1::DeleteBackward,
        ),
        (
            "strike Delete",
            "A ~~t~~ Z",
            4,
            DocumentEditIntentV1::DeleteForward,
        ),
        (
            "code Backspace",
            "A `t` Z",
            4,
            DocumentEditIntentV1::DeleteBackward,
        ),
        (
            "code Delete",
            "A `t` Z",
            3,
            DocumentEditIntentV1::DeleteForward,
        ),
        (
            "nested Backspace",
            "A ***t*** Z",
            6,
            DocumentEditIntentV1::DeleteBackward,
        ),
        (
            "nested Delete",
            "A ***t*** Z",
            5,
            DocumentEditIntentV1::DeleteForward,
        ),
        (
            "extended grapheme Backspace",
            "A *e\u{301}* Z",
            5,
            DocumentEditIntentV1::DeleteBackward,
        ),
        (
            "extended grapheme Delete",
            "A *e\u{301}* Z",
            3,
            DocumentEditIntentV1::DeleteForward,
        ),
        (
            "escaped literal Backspace",
            "A \\* Z",
            4,
            DocumentEditIntentV1::DeleteBackward,
        ),
        (
            "escaped literal Delete",
            "A \\* Z",
            3,
            DocumentEditIntentV1::DeleteForward,
        ),
        (
            "nested escaped emphasis Backspace",
            "A *\\** Z",
            5,
            DocumentEditIntentV1::DeleteBackward,
        ),
        (
            "nested escaped emphasis Delete",
            "A *\\** Z",
            4,
            DocumentEditIntentV1::DeleteForward,
        ),
        (
            "nested escaped strong Backspace",
            "A **\\*** Z",
            6,
            DocumentEditIntentV1::DeleteBackward,
        ),
        (
            "nested escaped strong Delete",
            "A **\\*** Z",
            5,
            DocumentEditIntentV1::DeleteForward,
        ),
    ];

    for (label, initial, selection_utf16, intent) in cases {
        let mut document = DocumentSession::begin(initial).expect(label);
        pump_ready(&mut document);
        let receipt = document
            .try_apply_edit_intent_v1(1, intent, selection_utf16, false)
            .expect(label);
        assert_eq!(
            receipt.disposition,
            DocumentEditIntentDispositionV1::Applied,
            "{label}"
        );
        assert_eq!(receipt.result_selection_utf16, 2, "{label}");
        if label.starts_with("escaped") {
            assert_eq!(
                receipt.inline_continuation, None,
                "an escape atom is not a persistent inline mode: {label}"
            );
            assert_eq!(source(&document), "A  Z", "{label}");
            document.close().expect(label);
            continue;
        }
        let recipe = receipt
            .inline_continuation
            .as_ref()
            .expect("continuable delete-to-empty carries parser authority");
        let (prefix, suffix, collisions) = if label.starts_with("nested escaped strong") {
            ("**", "**", "*\\")
        } else if label.starts_with("nested escaped emphasis") {
            ("*", "*", "*\\")
        } else if label.starts_with("strong") {
            ("**", "**", "*\\")
        } else if label.starts_with("strike") {
            ("~~", "~~", "\\~")
        } else if label.starts_with("code") {
            ("`", "`", "`")
        } else if label.starts_with("nested") {
            ("***", "***", "*\\")
        } else {
            ("*", "*", "*\\")
        };
        assert_eq!(recipe.prefix, prefix, "{label}");
        assert_eq!(recipe.suffix, suffix, "{label}");
        assert_eq!(recipe.collision_scalars, collisions, "{label}");
        assert_eq!(
            recipe.scalar_policy,
            DocumentInlineContinuationScalarPolicyV1::StableNonWhitespace,
            "{label}"
        );
        assert_eq!(source(&document), "A  Z", "{label}");
        document.close().expect(label);
    }
}

#[test]
fn inline_delete_to_empty_never_partially_removes_a_link_owner() {
    let mut document = DocumentSession::begin("A [*t*](url) Z").expect("begin nested link");
    pump_ready(&mut document);
    let receipt = document
        .try_apply_edit_intent_v1(1, DocumentEditIntentV1::DeleteBackward, 5, false)
        .expect("resolve link-owned emphasis Backspace");
    assert_eq!(
        receipt.disposition,
        DocumentEditIntentDispositionV1::NotApplicable
    );
    assert_eq!(source(&document), "A [*t*](url) Z");
    document.close().expect("close nested link");
}

#[test]
fn inline_continuation_distinguishes_ordinary_from_punctuation_flanking() {
    let mut document = DocumentSession::begin("A*t*Z").expect("begin intraword emphasis");
    pump_ready(&mut document);
    let receipt = document
        .try_apply_edit_intent_v1(1, DocumentEditIntentV1::DeleteBackward, 3, false)
        .expect("delete intraword emphasis owner");
    assert_eq!(
        receipt.disposition,
        DocumentEditIntentDispositionV1::Applied
    );
    assert_eq!(receipt.result_selection_utf16, 1);
    let recipe = receipt
        .inline_continuation
        .as_ref()
        .expect("intraword emphasis continues only ordinary scalars");
    assert_eq!(recipe.prefix, "*");
    assert_eq!(recipe.suffix, "*");
    assert_eq!(recipe.collision_scalars, "*\\");
    assert_eq!(
        recipe.scalar_policy,
        DocumentInlineContinuationScalarPolicyV1::CommonMarkOrdinaryOnly
    );
    assert_eq!(source(&document), "AZ");
    document.close().expect("close intraword emphasis");

    let mut punctuated = DocumentSession::begin("A(*t*)Z").expect("begin punctuated emphasis");
    pump_ready(&mut punctuated);
    let receipt = punctuated
        .try_apply_edit_intent_v1(1, DocumentEditIntentV1::DeleteBackward, 4, false)
        .expect("delete punctuation-bounded emphasis owner");
    assert_eq!(
        receipt
            .inline_continuation
            .as_ref()
            .map(|recipe| (recipe.prefix.as_str(), recipe.suffix.as_str())),
        Some(("*", "*"))
    );
    assert_eq!(
        receipt
            .inline_continuation
            .as_ref()
            .map(|recipe| recipe.scalar_policy),
        Some(DocumentInlineContinuationScalarPolicyV1::StableNonWhitespace)
    );
    punctuated.close().expect("close punctuated emphasis");
}

#[test]
fn inline_delete_to_empty_proof_does_not_claim_a_multi_grapheme_owner() {
    let mut document = DocumentSession::begin("A *ab* Z").expect("begin emphasis control");
    pump_ready(&mut document);
    let receipt = document
        .try_apply_edit_intent_v1(1, DocumentEditIntentV1::DeleteBackward, 5, false)
        .expect("resolve ordinary interior Backspace");
    assert_eq!(
        receipt.disposition,
        DocumentEditIntentDispositionV1::NotApplicable
    );
    assert_eq!(source(&document), "A *ab* Z");
    document.close().expect("close emphasis control");
}

#[test]
fn fenced_code_return_retains_presentation_only_when_the_new_suffix_cannot_close() {
    let mut terminal =
        DocumentSession::begin("```dart\nfinal value = 1;\n```\n").expect("begin fenced code");
    pump_ready(&mut terminal);
    let terminal_receipt = terminal
        .try_apply_edit_intent_v1(1, DocumentEditIntentV1::InsertParagraphBreak, 24, false)
        .expect("split terminal code line");
    assert_eq!(
        terminal_receipt.presentation_transition,
        DocumentEditPresentationTransitionV1::SplitParagraph
    );
    assert!(terminal_receipt.presentation_proven);
    assert_eq!(terminal_receipt.result_selection_utf16, 25);
    assert_eq!(source(&terminal), "```dart\nfinal value = 1;\n\n```\n");
    pump_ready(&mut terminal);
    let join_receipt = terminal
        .try_apply_edit_intent_v1(2, DocumentEditIntentV1::DeleteBackward, 25, false)
        .expect("join empty fenced line");
    assert_eq!(
        join_receipt.presentation_transition,
        DocumentEditPresentationTransitionV1::JoinFencedCode
    );
    assert!(join_receipt.presentation_proven);
    assert_eq!(join_receipt.result_selection_utf16, 24);
    assert_eq!(source(&terminal), "```dart\nfinal value = 1;\n```\n");
    terminal.close().expect("close fenced code");

    let mut unsafe_suffix =
        DocumentSession::begin("```\nabc```\n```\n").expect("begin fence-like suffix");
    pump_ready(&mut unsafe_suffix);
    let unsafe_receipt = unsafe_suffix
        .try_apply_edit_intent_v1(1, DocumentEditIntentV1::InsertParagraphBreak, 7, false)
        .expect("split before fence-like suffix");
    assert_eq!(
        unsafe_receipt.presentation_transition,
        DocumentEditPresentationTransitionV1::SplitParagraph
    );
    assert!(!unsafe_receipt.presentation_proven);
    assert_eq!(source(&unsafe_suffix), "```\nabc\n```\n```\n");
    unsafe_suffix.close().expect("close unsafe suffix");
}

#[test]
fn pending_fenced_context_follows_an_edit_back_to_the_predecessor_line() {
    let initial = "```dart\nfinal value = 1;\n```\n\n**sentinel**\n";
    let mut document = DocumentSession::begin(initial).expect("begin fenced predecessor edit");
    pump_ready(&mut document);

    let first = document
        .try_apply_edit_intent_v1(1, DocumentEditIntentV1::InsertParagraphBreak, 24, false)
        .expect("split fenced line");
    assert_eq!(first.result_selection_utf16, 25);
    document
        .apply_edit(first.result_revision, 24..24, "[")
        .expect("edit the known predecessor without pumping");
    document
        .apply_edit(3, 25..25, " ")
        .expect("continue typing on the predecessor without pumping");

    let second = document
        .try_apply_edit_intent_v1(4, DocumentEditIntentV1::InsertParagraphBreak, 26, false)
        .expect("split the edited predecessor without pumping");
    assert_eq!(
        second
            .committed_splice
            .expect("fenced split splice")
            .replacement,
        "\n"
    );
    assert_eq!(
        source(&document),
        "```dart\nfinal value = 1;[ \n\n\n```\n\n**sentinel**\n"
    );
    document.close().expect("close fenced predecessor edit");
}

#[test]
fn repeated_paragraph_breaks_are_parser_timing_independent() {
    let initial = "| a\n\n | b |\n| --- | --- |\n| c | d |\n\n**sentinel**\n";
    let expected = "| a\n\n\n\n | b |\n| --- | --- |\n| c | d |\n\n**sentinel**\n";

    for settle_between in [false, true] {
        let mut document = DocumentSession::begin(initial).expect("begin repeated breaks");
        pump_ready(&mut document);
        document
            .apply_edit(1, 5..5, "\n")
            .expect("third literal paragraph break");
        if settle_between {
            pump_ready(&mut document);
        }
        let fourth = document
            .try_apply_edit_intent_v1(2, DocumentEditIntentV1::InsertParagraphBreak, 6, false)
            .expect("fourth paragraph break");
        if let Some(splice) = fourth.committed_splice {
            assert_eq!(splice.replacement, "\n", "settle_between={settle_between}");
        } else {
            document
                .apply_edit(2, 6..6, "\n")
                .expect("fourth literal fallback");
        }
        assert_eq!(
            source(&document),
            expected,
            "settle_between={settle_between}"
        );
        document.close().expect("close repeated breaks");
    }
}

#[test]
fn settled_multi_row_gap_return_adds_one_editor_row() {
    let initial = "alpha\n\n\nnext\n";
    let expected = "alpha\n\n\n\nnext\n";
    let mut document = DocumentSession::begin(initial).expect("begin multi-row gap");
    pump_ready(&mut document);

    let inserted = document
        .try_apply_edit_intent_v1(1, DocumentEditIntentV1::InsertParagraphBreak, 8, false)
        .expect("insert one row at settled successor start");
    assert_eq!(
        inserted.disposition,
        DocumentEditIntentDispositionV1::Applied
    );
    assert_eq!(
        inserted
            .committed_splice
            .as_ref()
            .map(|splice| splice.replacement.as_str()),
        Some("\n")
    );
    assert_eq!(inserted.result_selection_utf16, 9);
    assert_eq!(source(&document), expected);
    document.close().expect("close multi-row gap");
}

#[test]
fn pending_multi_row_gap_backspace_removes_one_editor_row() {
    let initial = "alpha\n\n next\n";
    let pending = "alpha\n\n\nnext\n";
    let expected = "alpha\n\nnext\n";
    let mut document = DocumentSession::begin(initial).expect("begin pending multi-row gap");
    pump_ready(&mut document);
    document
        .apply_edit(1, 7..8, "\n")
        .expect("replace successor prefix with a newline");
    assert_eq!(source(&document), pending);
    assert_eq!(document.phase(), DocumentSessionPhase::Building);

    let removed = document
        .try_apply_edit_intent_v1(2, DocumentEditIntentV1::DeleteBackward, 8, false)
        .expect("remove only the nearest pending editor row");
    assert_eq!(
        removed.disposition,
        DocumentEditIntentDispositionV1::Applied
    );
    assert_eq!(removed.result_selection_utf16, 7);
    assert_eq!(source(&document), expected);
    document.close().expect("close pending multi-row gap");
}

#[test]
fn pending_bare_list_marker_return_exits_the_list() {
    let initial = "Plain text.\n\n\n\n**sentinel**\n";
    let mut document = DocumentSession::begin(initial).expect("begin paragraph gap");
    pump_ready(&mut document);
    document
        .apply_edit(1, 13..13, "*")
        .expect("insert bare list marker");
    assert_eq!(document.phase(), DocumentSessionPhase::Building);

    let receipt = document
        .try_apply_edit_intent_v1(2, DocumentEditIntentV1::InsertParagraphBreak, 14, false)
        .expect("Return on pending bare list marker");
    assert_eq!(
        receipt.disposition,
        DocumentEditIntentDispositionV1::Applied
    );
    assert_eq!(
        receipt.presentation_transition,
        DocumentEditPresentationTransitionV1::ExitList
    );
    assert_eq!(source(&document), initial);
    document.close().expect("close paragraph gap");
}

#[test]
fn settled_gap_backspace_reverses_one_added_paragraph_row() {
    let split_row = "| a\n | b |\n| --- | --- |\n| c | d |\n";
    let with_gap = "| a\n\n | b |\n| --- | --- |\n| c | d |\n";
    let mut document = DocumentSession::begin(with_gap).expect("begin split table gap");
    pump_ready(&mut document);

    let removed = document
        .try_apply_edit_intent_v1(1, DocumentEditIntentV1::DeleteBackward, 5, false)
        .expect("remove the added paragraph row");
    assert_eq!(
        removed.disposition,
        DocumentEditIntentDispositionV1::Applied
    );
    assert_eq!(source(&document), split_row);
    document.close().expect("close split table row");
}

#[test]
fn pending_gap_return_after_semantic_backspace_adds_one_editor_row() {
    let split_row = "| a\n | b |\n| --- | --- |\n| c | d |\n";
    let with_gap = "| a\n\n | b |\n| --- | --- |\n| c | d |\n";
    let mut document = DocumentSession::begin(with_gap).expect("begin split table gap");
    pump_ready(&mut document);

    let removed = document
        .try_apply_edit_intent_v1(1, DocumentEditIntentV1::DeleteBackward, 5, false)
        .expect("remove the added paragraph row");
    assert_eq!(
        removed.disposition,
        DocumentEditIntentDispositionV1::Applied
    );
    assert_eq!(removed.result_selection_utf16, 4);
    assert_eq!(source(&document), split_row);
    assert_eq!(document.phase(), DocumentSessionPhase::Building);

    let restored = document
        .try_apply_edit_intent_v1(2, DocumentEditIntentV1::InsertParagraphBreak, 4, false)
        .expect("restore one row before parser recertification");
    assert_eq!(
        restored.disposition,
        DocumentEditIntentDispositionV1::Applied
    );
    assert_eq!(restored.result_selection_utf16, 5);
    assert_eq!(source(&document), with_gap);
    document.close().expect("close split table row");
}

#[test]
fn settled_multiple_gap_backspace_removes_one_editor_row() {
    let initial = "- list item\n\n\n\n\n**sentinel**\n";
    let expected = "- list item\n\n\n\n**sentinel**\n";
    let mut document = DocumentSession::begin(initial).expect("begin repeated list gap");
    pump_ready(&mut document);

    let removed = document
        .try_apply_edit_intent_v1(1, DocumentEditIntentV1::DeleteBackward, 14, false)
        .expect("remove one unrepresented blank row");
    assert_eq!(
        removed.disposition,
        DocumentEditIntentDispositionV1::Applied
    );
    assert_eq!(removed.result_selection_utf16, 13);
    assert_eq!(source(&document), expected);
    document.close().expect("close repeated list gap");
}

#[test]
fn pending_whitespace_only_successor_return_adds_one_row() {
    let initial = "## Heading\n\n**sentinel**\n";
    let after_heading_return = "## Heading\n\n\n\n**sentinel**\n";
    let after_space = "## Heading\n\n \n\n**sentinel**\n";
    let expected = "## Heading\n\n \n\n\n**sentinel**\n";
    let mut document = DocumentSession::begin(initial).expect("begin heading");
    pump_ready(&mut document);

    let heading_return = document
        .try_apply_edit_intent_v1(1, DocumentEditIntentV1::InsertParagraphBreak, 10, false)
        .expect("split heading");
    assert_eq!(
        heading_return.disposition,
        DocumentEditIntentDispositionV1::Applied
    );
    assert_eq!(source(&document), after_heading_return);

    document
        .apply_edit(2, 12..12, " ")
        .expect("type whitespace in pending successor");
    assert_eq!(source(&document), after_space);
    assert_eq!(document.phase(), DocumentSessionPhase::Building);

    let blank_return = document
        .try_apply_edit_intent_v1(3, DocumentEditIntentV1::InsertParagraphBreak, 13, false)
        .expect("Return on whitespace-only successor");
    assert_eq!(
        blank_return.disposition,
        DocumentEditIntentDispositionV1::Applied
    );
    assert_eq!(blank_return.result_selection_utf16, 14);
    assert_eq!(source(&document), expected);
    document.close().expect("close whitespace successor");
}

#[test]
fn pending_heading_trailing_whitespace_does_not_extend_editable_context() {
    let mut document =
        DocumentSession::begin("## Heading\n\n**sentinel**\n").expect("begin heading");
    pump_ready(&mut document);

    document
        .apply_edit(1, 10..10, " ")
        .expect("append heading whitespace");
    assert_eq!(document.phase(), DocumentSessionPhase::Building);
    let receipt = document
        .try_apply_edit_intent_v1(2, DocumentEditIntentV1::InsertParagraphBreak, 11, false)
        .expect("classify Return after trailing whitespace");
    assert_eq!(
        receipt.disposition,
        DocumentEditIntentDispositionV1::NeedsCurrentSemantics
    );
    assert_eq!(source(&document), "## Heading \n\n**sentinel**\n");
    document.close().expect("close trailing whitespace heading");
}

#[test]
fn structural_presentation_proof_is_parser_bounded_and_fails_closed() {
    let mut split = DocumentSession::begin("Before **bold**.\n").expect("begin split fixture");
    pump_ready(&mut split);
    let split_receipt = split
        .try_apply_edit_intent_v1(
            1,
            DocumentEditIntentV1::InsertParagraphBreak,
            "Before **bold**.".len(),
            false,
        )
        .expect("split paragraph");
    assert_eq!(
        split_receipt.presentation_transition,
        DocumentEditPresentationTransitionV1::SplitParagraph
    );
    assert!(split_receipt.presentation_proven);
    split.close().expect("close split fixture");

    let mut heading = DocumentSession::begin("## Head").expect("begin heading split fixture");
    pump_ready(&mut heading);
    let heading_receipt = heading
        .try_apply_edit_intent_v1(
            1,
            DocumentEditIntentV1::InsertParagraphBreak,
            "## Head".len(),
            false,
        )
        .expect("split heading");
    assert_eq!(
        heading_receipt.presentation_transition,
        DocumentEditPresentationTransitionV1::SplitParagraph
    );
    assert!(heading_receipt.presentation_proven);
    heading.close().expect("close heading split fixture");

    let mut continuation =
        DocumentSession::begin("- list item\n").expect("begin terminal list continuation fixture");
    pump_ready(&mut continuation);
    let continuation_receipt = continuation
        .try_apply_edit_intent_v1(
            1,
            DocumentEditIntentV1::InsertParagraphBreak,
            "- list item".len(),
            false,
        )
        .expect("continue terminal list item");
    assert_eq!(
        continuation_receipt.presentation_transition,
        DocumentEditPresentationTransitionV1::ContinueList
    );
    assert!(continuation_receipt.presentation_proven);
    continuation
        .close()
        .expect("close terminal list continuation fixture");

    let mut exit = DocumentSession::begin("- one\n- \n").expect("begin list exit fixture");
    pump_ready(&mut exit);
    let exit_receipt = exit
        .try_apply_edit_intent_v1(
            1,
            DocumentEditIntentV1::InsertParagraphBreak,
            "- one\n- ".len(),
            false,
        )
        .expect("exit empty list item");
    assert_eq!(
        exit_receipt.presentation_transition,
        DocumentEditPresentationTransitionV1::ExitList
    );
    assert!(exit_receipt.presentation_proven);
    exit.close().expect("close list exit fixture");

    let mut quote = DocumentSession::begin("> quote\n").expect("begin quote fixture");
    pump_ready(&mut quote);
    let quote_continuation = quote
        .try_apply_edit_intent_v1(
            1,
            DocumentEditIntentV1::InsertParagraphBreak,
            "> quote".len(),
            false,
        )
        .expect("continue quote");
    assert_eq!(
        quote_continuation.presentation_transition,
        DocumentEditPresentationTransitionV1::ContinueBlockQuote
    );
    assert!(quote_continuation.presentation_proven);
    pump_ready(&mut quote);
    let quote_exit = quote
        .try_apply_edit_intent_v1(
            2,
            DocumentEditIntentV1::InsertParagraphBreak,
            "> quote\n> ".len(),
            false,
        )
        .expect("exit empty quote");
    assert_eq!(
        quote_exit.presentation_transition,
        DocumentEditPresentationTransitionV1::ExitBlockQuote
    );
    assert!(quote_exit.presentation_proven);
    quote.close().expect("close quote fixture");

    for source in ["## Head ##", "## Head   "] {
        let mut suffixed = DocumentSession::begin(source).expect("begin suffixed heading");
        pump_ready(&mut suffixed);
        let receipt = suffixed
            .try_apply_edit_intent_v1(
                1,
                DocumentEditIntentV1::InsertParagraphBreak,
                "## Head".len(),
                false,
            )
            .expect("split suffixed heading");
        assert_eq!(
            receipt.presentation_transition,
            DocumentEditPresentationTransitionV1::SplitParagraph
        );
        assert!(!receipt.presentation_proven, "{source:?}");
        suffixed.close().expect("close suffixed heading");
    }

    let merge_source = "Before **bold**.\n\nAfter.\n";
    let mut merge = DocumentSession::begin(merge_source).expect("begin merge fixture");
    pump_ready(&mut merge);
    let merge_receipt = merge
        .try_apply_edit_intent_v1(
            1,
            DocumentEditIntentV1::DeleteBackward,
            "Before **bold**.\n\n".len(),
            false,
        )
        .expect("merge paragraphs");
    assert_eq!(
        merge_receipt.presentation_transition,
        DocumentEditPresentationTransitionV1::MergeParagraph
    );
    assert!(merge_receipt.presentation_proven);
    merge.close().expect("close merge fixture");

    let crossing_source = "Before *\n\nafter*\n";
    let mut crossing = DocumentSession::begin(crossing_source).expect("begin crossing fixture");
    pump_ready(&mut crossing);
    let crossing_receipt = crossing
        .try_apply_edit_intent_v1(
            1,
            DocumentEditIntentV1::DeleteBackward,
            "Before *\n\n".len(),
            false,
        )
        .expect("merge crossing delimiter paragraphs");
    assert!(!crossing_receipt.presentation_proven);
    crossing.close().expect("close crossing fixture");

    let mut pending = DocumentSession::begin("Before **bold**.\n").expect("begin pending fixture");
    let pending_receipt = pending
        .try_apply_edit_intent_v1(
            1,
            DocumentEditIntentV1::InsertParagraphBreak,
            "Before **bold**.".len(),
            false,
        )
        .expect("pending split paragraph");
    assert!(!pending_receipt.presentation_proven);
    pending.close().expect("close pending fixture");

    let mut list = DocumentSession::begin("- parent\n- child\n").expect("begin list fixture");
    pump_ready(&mut list);
    let indent_receipt = list
        .try_apply_edit_intent_v1(1, DocumentEditIntentV1::IndentListItem, 16, false)
        .expect("indent list item");
    assert_eq!(
        indent_receipt.presentation_transition,
        DocumentEditPresentationTransitionV1::IndentList
    );
    assert!(indent_receipt.presentation_proven);
    pump_ready(&mut list);
    let outdent_receipt = list
        .try_apply_edit_intent_v1(2, DocumentEditIntentV1::OutdentListItem, 18, false)
        .expect("outdent list item");
    assert_eq!(
        outdent_receipt.presentation_transition,
        DocumentEditPresentationTransitionV1::OutdentList
    );
    assert!(outdent_receipt.presentation_proven);
    list.close().expect("close list fixture");

    let mut pending_list =
        DocumentSession::begin("- parent\n- child\n").expect("begin pending list fixture");
    let pending_list_receipt = pending_list
        .try_apply_edit_intent_v1(1, DocumentEditIntentV1::IndentListItem, 16, false)
        .expect("pending list indent");
    assert!(!pending_list_receipt.presentation_proven);
    pending_list.close().expect("close pending list fixture");
}

#[test]
fn return_at_embedded_plain_line_start_uses_one_typed_split() {
    let initial = "| a\n | b |\n| --- | --- |\n| c | d |\n\n**sentinel**\n";
    let mut document = DocumentSession::begin(initial).expect("begin multiline paragraph fixture");
    pump_ready(&mut document);
    let receipt = document
        .try_apply_edit_intent_v1(1, DocumentEditIntentV1::InsertParagraphBreak, 4, false)
        .expect("split at embedded physical line start");
    assert_eq!(
        receipt.disposition,
        DocumentEditIntentDispositionV1::Applied
    );
    assert_eq!(
        receipt.presentation_transition,
        DocumentEditPresentationTransitionV1::SplitParagraph
    );
    assert!(receipt.presentation_proven);
    assert_eq!(receipt.committed_splice.as_ref().unwrap().replacement, "\n");
    assert_eq!(
        source(&document),
        "| a\n\n | b |\n| --- | --- |\n| c | d |\n\n**sentinel**\n"
    );
    document.close().expect("close multiline paragraph fixture");
}

#[derive(Clone, Copy)]
struct IntentCase {
    name: &'static str,
    initial: &'static str,
    intent: DocumentEditIntentV1,
    selection_utf16: usize,
    expected: &'static str,
    expected_selection_utf16: usize,
    expected_transition: DocumentEditPresentationTransitionV1,
}

#[test]
fn collapsed_e1_matrix_commits_one_exact_splice() {
    let cases = [
        IntentCase {
            name: "plain Return",
            initial: "alpha bravo\n",
            intent: DocumentEditIntentV1::InsertParagraphBreak,
            selection_utf16: 5,
            expected: "alpha\n\n bravo\n",
            expected_selection_utf16: 7,
            expected_transition: DocumentEditPresentationTransitionV1::SplitParagraph,
        },
        IntentCase {
            name: "terminated paragraph boundary Return creates a distinct editor row",
            initial: "alpha\n\nnext\n",
            intent: DocumentEditIntentV1::InsertParagraphBreak,
            selection_utf16: 5,
            expected: "alpha\n\n\n\nnext\n",
            expected_selection_utf16: 7,
            expected_transition: DocumentEditPresentationTransitionV1::SplitParagraph,
        },
        IntentCase {
            name: "ATX heading Return creates a plain successor",
            initial: "# head",
            intent: DocumentEditIntentV1::InsertParagraphBreak,
            selection_utf16: 6,
            expected: "# head\n\n",
            expected_selection_utf16: 8,
            expected_transition: DocumentEditPresentationTransitionV1::SplitParagraph,
        },
        IntentCase {
            name: "empty ATX heading exits",
            initial: "# ",
            intent: DocumentEditIntentV1::InsertParagraphBreak,
            selection_utf16: 2,
            expected: "\n",
            expected_selection_utf16: 1,
            expected_transition: DocumentEditPresentationTransitionV1::ExitHeading,
        },
        IntentCase {
            name: "indented empty ATX heading preserves indentation",
            initial: "  ## ",
            intent: DocumentEditIntentV1::InsertParagraphBreak,
            selection_utf16: 5,
            expected: "  \n",
            expected_selection_utf16: 3,
            expected_transition: DocumentEditPresentationTransitionV1::ExitHeading,
        },
        IntentCase {
            name: "ATX heading prefix lifts",
            initial: "## Head\n",
            intent: DocumentEditIntentV1::DeleteBackward,
            selection_utf16: 3,
            expected: "Head\n",
            expected_selection_utf16: 0,
            expected_transition: DocumentEditPresentationTransitionV1::LiftHeading,
        },
        IntentCase {
            name: "simple quote continues",
            initial: "> alpha",
            intent: DocumentEditIntentV1::InsertParagraphBreak,
            selection_utf16: 7,
            expected: "> alpha\n> ",
            expected_selection_utf16: 10,
            expected_transition: DocumentEditPresentationTransitionV1::ContinueBlockQuote,
        },
        IntentCase {
            name: "projected multiline quote continues its physical line",
            initial: "> first\n> second\n",
            intent: DocumentEditIntentV1::InsertParagraphBreak,
            selection_utf16: 13,
            expected: "> first\n> sec\n> ond\n",
            expected_selection_utf16: 16,
            expected_transition: DocumentEditPresentationTransitionV1::ContinueBlockQuote,
        },
        IntentCase {
            name: "projected multiline quote lifts one physical line",
            initial: "> first\n> second\n",
            intent: DocumentEditIntentV1::DeleteBackward,
            selection_utf16: 10,
            expected: "> first\n\nsecond\n",
            expected_selection_utf16: 9,
            expected_transition: DocumentEditPresentationTransitionV1::LiftBlockQuote,
        },
        IntentCase {
            name: "certified empty multiline quote line exits",
            initial: "> first\n> \n",
            intent: DocumentEditIntentV1::InsertParagraphBreak,
            selection_utf16: 10,
            expected: "> first\n\n\n",
            expected_selection_utf16: 9,
            expected_transition: DocumentEditPresentationTransitionV1::ExitBlockQuote,
        },
        IntentCase {
            name: "certified CRLF empty multiline quote line exits",
            initial: "> first\r\n> \r\n",
            intent: DocumentEditIntentV1::InsertParagraphBreak,
            selection_utf16: 11,
            expected: "> first\r\n\r\n\r\n",
            expected_selection_utf16: 11,
            expected_transition: DocumentEditPresentationTransitionV1::ExitBlockQuote,
        },
        IntentCase {
            name: "empty quote exits",
            initial: "> ",
            intent: DocumentEditIntentV1::InsertParagraphBreak,
            selection_utf16: 2,
            expected: "\n",
            expected_selection_utf16: 1,
            expected_transition: DocumentEditPresentationTransitionV1::ExitBlockQuote,
        },
        IntentCase {
            name: "quote prefix lifts",
            initial: "> alpha\n",
            intent: DocumentEditIntentV1::DeleteBackward,
            selection_utf16: 2,
            expected: "alpha\n",
            expected_selection_utf16: 0,
            expected_transition: DocumentEditPresentationTransitionV1::LiftBlockQuote,
        },
        IntentCase {
            name: "nested quote continues with its exact parser prefix",
            initial: "> > alpha",
            intent: DocumentEditIntentV1::InsertParagraphBreak,
            selection_utf16: 9,
            expected: "> > alpha\n> > ",
            expected_selection_utf16: 14,
            expected_transition: DocumentEditPresentationTransitionV1::ContinueBlockQuote,
        },
        IntentCase {
            name: "nested quote Backspace outdents one container",
            initial: "> > alpha\n",
            intent: DocumentEditIntentV1::DeleteBackward,
            selection_utf16: 4,
            expected: "> alpha\n",
            expected_selection_utf16: 2,
            expected_transition: DocumentEditPresentationTransitionV1::OutdentBlockQuote,
        },
        IntentCase {
            name: "empty nested quote Return outdents one container",
            initial: "> > ",
            intent: DocumentEditIntentV1::InsertParagraphBreak,
            selection_utf16: 4,
            expected: "> ",
            expected_selection_utf16: 2,
            expected_transition: DocumentEditPresentationTransitionV1::OutdentBlockQuote,
        },
        IntentCase {
            name: "multiline nested quote outdents only the active physical line",
            initial: "> > first\n> > second\n",
            intent: DocumentEditIntentV1::DeleteBackward,
            selection_utf16: 14,
            expected: "> > first\n\n> second\n",
            expected_selection_utf16: 13,
            expected_transition: DocumentEditPresentationTransitionV1::OutdentBlockQuote,
        },
        IntentCase {
            name: "multiline CRLF nested quote preserves its line ending while outdenting",
            initial: "> > first\r\n> > second\r\n",
            intent: DocumentEditIntentV1::DeleteBackward,
            selection_utf16: 15,
            expected: "> > first\r\n\r\n> second\r\n",
            expected_selection_utf16: 15,
            expected_transition: DocumentEditPresentationTransitionV1::OutdentBlockQuote,
        },
        IntentCase {
            name: "unordered continuation",
            initial: "- alpha\n",
            intent: DocumentEditIntentV1::InsertParagraphBreak,
            selection_utf16: 7,
            expected: "- alpha\n- \n",
            expected_selection_utf16: 10,
            expected_transition: DocumentEditPresentationTransitionV1::ContinueList,
        },
        IntentCase {
            name: "ordered continuation",
            initial: "9) alpha\n",
            intent: DocumentEditIntentV1::InsertParagraphBreak,
            selection_utf16: 8,
            expected: "9) alpha\n10) \n",
            expected_selection_utf16: 13,
            expected_transition: DocumentEditPresentationTransitionV1::ContinueList,
        },
        IntentCase {
            name: "Tab indents a later bullet beneath its preceding sibling",
            initial: "- parent\n- child\n",
            intent: DocumentEditIntentV1::IndentListItem,
            selection_utf16: 16,
            expected: "- parent\n  - child\n",
            expected_selection_utf16: 18,
            expected_transition: DocumentEditPresentationTransitionV1::IndentList,
        },
        IntentCase {
            name: "Shift-Tab outdents a nested bullet without moving to its prefix",
            initial: "- parent\n  - child\n",
            intent: DocumentEditIntentV1::OutdentListItem,
            selection_utf16: 18,
            expected: "- parent\n- child\n",
            expected_selection_utf16: 16,
            expected_transition: DocumentEditPresentationTransitionV1::OutdentList,
        },
        IntentCase {
            name: "ordered indentation uses the preceding item padding",
            initial: "10. parent\n11. child\n",
            intent: DocumentEditIntentV1::IndentListItem,
            selection_utf16: 20,
            expected: "10. parent\n    11. child\n",
            expected_selection_utf16: 24,
            expected_transition: DocumentEditPresentationTransitionV1::IndentList,
        },
        IntentCase {
            name: "task indentation preserves the certified check state",
            initial: "- [ ] parent\n- [x] child\n",
            intent: DocumentEditIntentV1::IndentListItem,
            selection_utf16: 24,
            expected: "- [ ] parent\n  - [x] child\n",
            expected_selection_utf16: 26,
            expected_transition: DocumentEditPresentationTransitionV1::IndentList,
        },
        IntentCase {
            name: "indentation finds a preceding sibling across its nested subtree",
            initial: "- first\n  - nested\n- second\n",
            intent: DocumentEditIntentV1::IndentListItem,
            selection_utf16: 27,
            expected: "- first\n  - nested\n  - second\n",
            expected_selection_utf16: 29,
            expected_transition: DocumentEditPresentationTransitionV1::IndentList,
        },
        IntentCase {
            name: "depth-two continuation preserves its container indentation",
            initial: "- parent\n  - child\n",
            intent: DocumentEditIntentV1::InsertParagraphBreak,
            selection_utf16: 18,
            expected: "- parent\n  - child\n  - \n",
            expected_selection_utf16: 23,
            expected_transition: DocumentEditPresentationTransitionV1::ContinueList,
        },
        IntentCase {
            name: "depth-two prefix Backspace outdents one level",
            initial: "- parent\n  - child\n",
            intent: DocumentEditIntentV1::DeleteBackward,
            selection_utf16: 13,
            expected: "- parent\n- child\n",
            expected_selection_utf16: 11,
            expected_transition: DocumentEditPresentationTransitionV1::OutdentList,
        },
        IntentCase {
            name: "certified empty depth-two successor outdents one level",
            initial: "- parent\n  - child\n  - \n",
            intent: DocumentEditIntentV1::InsertParagraphBreak,
            selection_utf16: 23,
            expected: "- parent\n  - child\n- \n",
            expected_selection_utf16: 21,
            expected_transition: DocumentEditPresentationTransitionV1::OutdentList,
        },
        IntentCase {
            name: "depth-three continuation preserves its container indentation",
            initial: "- root\n  - child\n    - leaf\n",
            intent: DocumentEditIntentV1::InsertParagraphBreak,
            selection_utf16: 27,
            expected: "- root\n  - child\n    - leaf\n    - \n",
            expected_selection_utf16: 34,
            expected_transition: DocumentEditPresentationTransitionV1::ContinueList,
        },
        IntentCase {
            name: "depth-three prefix Backspace outdents exactly one level",
            initial: "- root\n  - child\n    - leaf\n",
            intent: DocumentEditIntentV1::DeleteBackward,
            selection_utf16: 23,
            expected: "- root\n  - child\n  - leaf\n",
            expected_selection_utf16: 21,
            expected_transition: DocumentEditPresentationTransitionV1::OutdentList,
        },
        IntentCase {
            name: "unchecked task continuation",
            initial: "- [ ] alpha\n",
            intent: DocumentEditIntentV1::InsertParagraphBreak,
            selection_utf16: 11,
            expected: "- [ ] alpha\n- [ ] \n",
            expected_selection_utf16: 18,
            expected_transition: DocumentEditPresentationTransitionV1::ContinueList,
        },
        IntentCase {
            name: "checked task continues unchecked",
            initial: "- [x] done\n",
            intent: DocumentEditIntentV1::InsertParagraphBreak,
            selection_utf16: 10,
            expected: "- [x] done\n- [ ] \n",
            expected_selection_utf16: 17,
            expected_transition: DocumentEditPresentationTransitionV1::ContinueList,
        },
        IntentCase {
            name: "empty task exits",
            initial: "- [ ] \n",
            intent: DocumentEditIntentV1::InsertParagraphBreak,
            selection_utf16: 6,
            expected: "\n",
            expected_selection_utf16: 0,
            expected_transition: DocumentEditPresentationTransitionV1::ExitList,
        },
        IntentCase {
            name: "empty list exits",
            initial: "- \n",
            intent: DocumentEditIntentV1::InsertParagraphBreak,
            selection_utf16: 2,
            expected: "\n",
            expected_selection_utf16: 0,
            expected_transition: DocumentEditPresentationTransitionV1::ExitList,
        },
        IntentCase {
            name: "unterminated empty list exits onto a blank line",
            initial: "- ",
            intent: DocumentEditIntentV1::InsertParagraphBreak,
            selection_utf16: 2,
            expected: "\n",
            expected_selection_utf16: 1,
            expected_transition: DocumentEditPresentationTransitionV1::ExitList,
        },
        IntentCase {
            name: "later empty list exits into a separated paragraph",
            initial: "- one\n- \n",
            intent: DocumentEditIntentV1::InsertParagraphBreak,
            selection_utf16: 8,
            expected: "- one\n\n\n",
            expected_selection_utf16: 7,
            expected_transition: DocumentEditPresentationTransitionV1::ExitList,
        },
        IntentCase {
            name: "standalone empty list preserves existing separation",
            initial: "para\n\n- \n",
            intent: DocumentEditIntentV1::InsertParagraphBreak,
            selection_utf16: 8,
            expected: "para\n\n\n",
            expected_selection_utf16: 6,
            expected_transition: DocumentEditPresentationTransitionV1::ExitList,
        },
        IntentCase {
            name: "first list item lifts",
            initial: "- alpha\n",
            intent: DocumentEditIntentV1::DeleteBackward,
            selection_utf16: 2,
            expected: "alpha\n",
            expected_selection_utf16: 0,
            expected_transition: DocumentEditPresentationTransitionV1::LiftList,
        },
        IntentCase {
            name: "task item lifts",
            initial: "- [X] alpha\n",
            intent: DocumentEditIntentV1::DeleteBackward,
            selection_utf16: 6,
            expected: "alpha\n",
            expected_selection_utf16: 0,
            expected_transition: DocumentEditPresentationTransitionV1::LiftList,
        },
        IntentCase {
            name: "later list item lifts with separation",
            initial: "- one\n- two\n",
            intent: DocumentEditIntentV1::DeleteBackward,
            selection_utf16: 8,
            expected: "- one\n\ntwo\n",
            expected_selection_utf16: 7,
            expected_transition: DocumentEditPresentationTransitionV1::LiftList,
        },
        IntentCase {
            name: "plain paragraph merge",
            initial: "one\n\ntwo\n",
            intent: DocumentEditIntentV1::DeleteBackward,
            selection_utf16: 5,
            expected: "onetwo\n",
            expected_selection_utf16: 3,
            expected_transition: DocumentEditPresentationTransitionV1::MergeParagraph,
        },
        IntentCase {
            name: "CRLF paragraph Return",
            initial: "alpha\r\n",
            intent: DocumentEditIntentV1::InsertParagraphBreak,
            selection_utf16: 5,
            expected: "alpha\r\n\r\n\r\n",
            expected_selection_utf16: 9,
            expected_transition: DocumentEditPresentationTransitionV1::SplitParagraph,
        },
        IntentCase {
            name: "indented code Return preserves the hidden prefix",
            initial: "    code\n",
            intent: DocumentEditIntentV1::InsertParagraphBreak,
            selection_utf16: 8,
            expected: "    code\n    \n",
            expected_selection_utf16: 13,
            expected_transition: DocumentEditPresentationTransitionV1::ContinueIndentedCode,
        },
        IntentCase {
            name: "indented code Backspace joins visible lines and hidden prefix",
            initial: "    one\n    two\n",
            intent: DocumentEditIntentV1::DeleteBackward,
            selection_utf16: 12,
            expected: "    onetwo\n",
            expected_selection_utf16: 7,
            expected_transition: DocumentEditPresentationTransitionV1::JoinIndentedCode,
        },
        IntentCase {
            name: "first indented code line Backspace lifts to plain text",
            initial: "    code\n",
            intent: DocumentEditIntentV1::DeleteBackward,
            selection_utf16: 4,
            expected: "code\n",
            expected_selection_utf16: 0,
            expected_transition: DocumentEditPresentationTransitionV1::LiftIndentedCode,
        },
        IntentCase {
            name: "CRLF indented code Return preserves the hidden prefix",
            initial: "    code\r\n",
            intent: DocumentEditIntentV1::InsertParagraphBreak,
            selection_utf16: 8,
            expected: "    code\r\n    \r\n",
            expected_selection_utf16: 14,
            expected_transition: DocumentEditPresentationTransitionV1::ContinueIndentedCode,
        },
        IntentCase {
            name: "tab-indented code Return repeats the parser-owned tab",
            initial: "\tcode\n",
            intent: DocumentEditIntentV1::InsertParagraphBreak,
            selection_utf16: 5,
            expected: "\tcode\n\t\n",
            expected_selection_utf16: 7,
            expected_transition: DocumentEditPresentationTransitionV1::ContinueIndentedCode,
        },
        IntentCase {
            name: "mixed space-tab code Return repeats the exact four-column prefix",
            initial: "  \tcode\n",
            intent: DocumentEditIntentV1::InsertParagraphBreak,
            selection_utf16: 7,
            expected: "  \tcode\n  \t\n",
            expected_selection_utf16: 11,
            expected_transition: DocumentEditPresentationTransitionV1::ContinueIndentedCode,
        },
        IntentCase {
            name: "residual code indentation remains visible while Return adds four columns",
            initial: "      code\n",
            intent: DocumentEditIntentV1::InsertParagraphBreak,
            selection_utf16: 10,
            expected: "      code\n    \n",
            expected_selection_utf16: 15,
            expected_transition: DocumentEditPresentationTransitionV1::ContinueIndentedCode,
        },
        IntentCase {
            name: "BOF BOM is not repeated by indented code Return",
            initial: "\u{feff}    code\n",
            intent: DocumentEditIntentV1::InsertParagraphBreak,
            selection_utf16: 9,
            expected: "\u{feff}    code\n    \n",
            expected_selection_utf16: 14,
            expected_transition: DocumentEditPresentationTransitionV1::ContinueIndentedCode,
        },
        IntentCase {
            name: "thematic break Backspace deletes the whole physical atom",
            initial: "---\nnext",
            intent: DocumentEditIntentV1::DeleteBackward,
            selection_utf16: 0,
            expected: "next",
            expected_selection_utf16: 0,
            expected_transition: DocumentEditPresentationTransitionV1::DeleteThematicBreak,
        },
        IntentCase {
            name: "thematic break Delete deletes spaces tabs and CRLF atomically",
            initial: "  * \t* *  \r\nnext",
            intent: DocumentEditIntentV1::DeleteForward,
            selection_utf16: 0,
            expected: "next",
            expected_selection_utf16: 0,
            expected_transition: DocumentEditPresentationTransitionV1::DeleteThematicBreak,
        },
        IntentCase {
            name: "thematic break deletion preserves a BOF BOM",
            initial: "\u{feff}---\nnext",
            intent: DocumentEditIntentV1::DeleteBackward,
            selection_utf16: 1,
            expected: "\u{feff}next",
            expected_selection_utf16: 1,
            expected_transition: DocumentEditPresentationTransitionV1::DeleteThematicBreak,
        },
    ];

    for case in cases {
        let mut document = DocumentSession::begin(case.initial).expect(case.name);
        pump_ready(&mut document);
        let receipt = document
            .try_apply_edit_intent_v1(1, case.intent, case.selection_utf16, false)
            .unwrap_or_else(|error| panic!("{} failed: {error:?}", case.name));
        assert_eq!(
            receipt.disposition,
            DocumentEditIntentDispositionV1::Applied,
            "{}",
            case.name
        );
        assert_eq!(receipt.base_revision, 1, "{}", case.name);
        assert_eq!(receipt.result_revision, 2, "{}", case.name);
        assert!(receipt.committed_splice.is_some(), "{}", case.name);
        assert_eq!(
            receipt.presentation_transition, case.expected_transition,
            "{}",
            case.name
        );
        assert_eq!(
            receipt.result_selection_utf16, case.expected_selection_utf16,
            "{}",
            case.name
        );
        assert_eq!(source(&document), case.expected, "{}", case.name);
        document.close().expect("close fixture");
    }
}

#[test]
fn list_indentation_boundary_commands_are_handled_without_mutation() {
    for (name, source_text, intent, selection) in [
        (
            "first item cannot indent",
            "- first\n- second\n",
            DocumentEditIntentV1::IndentListItem,
            7,
        ),
        (
            "top-level item cannot outdent",
            "- first\n- second\n",
            DocumentEditIntentV1::OutdentListItem,
            16,
        ),
    ] {
        let mut document = DocumentSession::begin(source_text).expect(name);
        pump_ready(&mut document);
        let receipt = document
            .try_apply_edit_intent_v1(1, intent, selection, false)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
        assert_eq!(
            receipt.disposition,
            DocumentEditIntentDispositionV1::HandledNoChange,
            "{name}"
        );
        assert_eq!(document.revision(), 1, "{name}");
        assert_eq!(source(&document), source_text, "{name}");
        document.close().expect("close boundary fixture");
    }
}

#[test]
fn list_indent_never_partially_moves_a_multiline_item_or_its_subtree() {
    for (name, source_text) in [
        (
            "multiline item",
            "- previous\n- current first\n  continuation\n",
        ),
        (
            "item with a nested child",
            "- previous\n- current\n  - child\n",
        ),
    ] {
        let selection = source_text.find("current").expect("current item") + "current".len();
        let mut document = DocumentSession::begin(source_text).expect(name);
        pump_ready(&mut document);
        let receipt = document
            .try_apply_edit_intent_v1(1, DocumentEditIntentV1::IndentListItem, selection, false)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
        assert_ne!(
            receipt.disposition,
            DocumentEditIntentDispositionV1::Applied,
            "{name} must not move only its first physical line"
        );
        assert_eq!(document.revision(), 1, "{name}");
        assert_eq!(source(&document), source_text, "{name}");
        document.close().expect("close multiline list fixture");
    }
}

#[test]
fn complex_context_fails_closed_and_composition_never_mutates() {
    for initial in [
        "> - nested\n",
        "> # child heading\n",
        "> ---\n",
        "Setext\n---\n",
        "```\ncode\n```\n",
    ] {
        let mut document = DocumentSession::begin(initial).expect("begin complex fixture");
        pump_ready(&mut document);
        let before = source(&document);
        let receipt = document
            .try_apply_edit_intent_v1(1, DocumentEditIntentV1::InsertParagraphBreak, 0, false)
            .expect("resolve complex fixture");
        assert_ne!(
            receipt.disposition,
            DocumentEditIntentDispositionV1::Applied,
            "{initial:?}"
        );
        assert_eq!(document.revision(), 1, "{initial:?}");
        assert_eq!(source(&document), before, "{initial:?}");
        document.close().expect("close complex fixture");
    }

    let mut document = DocumentSession::begin("alpha\n").expect("begin composition fixture");
    pump_ready(&mut document);
    let receipt = document
        .try_apply_edit_intent_v1(1, DocumentEditIntentV1::InsertParagraphBreak, 5, true)
        .expect("composition suppression");
    assert_eq!(
        receipt.disposition,
        DocumentEditIntentDispositionV1::NotApplicable
    );
    assert_eq!(document.revision(), 1);
    assert_eq!(source(&document), "alpha\n");
    document.close().expect("close composition fixture");
}

#[test]
fn parser_pending_lineage_resolves_without_pumping() {
    let mut document = DocumentSession::begin("- a\n").expect("begin List fixture");
    pump_ready(&mut document);

    document
        .apply_edit(1, 3..3, "b")
        .expect("literal edit before Return");
    assert_eq!(document.phase(), DocumentSessionPhase::Building);
    let first = document
        .try_apply_edit_intent_v1(2, DocumentEditIntentV1::InsertParagraphBreak, 4, false)
        .expect("pending List Return");
    assert_eq!(first.disposition, DocumentEditIntentDispositionV1::Applied);
    assert_eq!(source(&document), "- ab\n- \n");

    document
        .apply_edit(
            3,
            first.result_selection_utf16..first.result_selection_utf16,
            "x",
        )
        .expect("type into native-created pending item");
    let second = document
        .try_apply_edit_intent_v1(
            4,
            DocumentEditIntentV1::InsertParagraphBreak,
            first.result_selection_utf16 + 1,
            false,
        )
        .expect("second pending List Return");
    assert_eq!(second.disposition, DocumentEditIntentDispositionV1::Applied);
    assert_eq!(source(&document), "- ab\n- x\n- \n");
    document.close().expect("close pending fixture");
}

#[test]
fn blank_paragraph_returns_and_backspaces_one_editor_row_per_command() {
    let initial = "alpha\n\nnext\n";
    let mut document = DocumentSession::begin(initial).expect("begin paragraph gap");
    pump_ready(&mut document);

    let first = document
        .try_apply_edit_intent_v1(1, DocumentEditIntentV1::InsertParagraphBreak, 5, false)
        .expect("first boundary Return");
    assert_eq!(first.result_selection_utf16, 7);
    assert_eq!(source(&document), "alpha\n\n\n\nnext\n");

    let second = document
        .try_apply_edit_intent_v1(
            2,
            DocumentEditIntentV1::InsertParagraphBreak,
            first.result_selection_utf16,
            false,
        )
        .expect("second pending blank Return");
    assert_eq!(second.result_selection_utf16, 8);
    assert_eq!(source(&document), "alpha\n\n\n\n\nnext\n");

    let remove_second = document
        .try_apply_edit_intent_v1(
            3,
            DocumentEditIntentV1::DeleteBackward,
            second.result_selection_utf16,
            false,
        )
        .expect("remove second blank row");
    assert_eq!(
        remove_second.presentation_transition,
        DocumentEditPresentationTransitionV1::RetainParagraphGap
    );
    assert_eq!(remove_second.result_selection_utf16, 7);
    assert_eq!(source(&document), "alpha\n\n\n\nnext\n");

    let remove_first = document
        .try_apply_edit_intent_v1(
            4,
            DocumentEditIntentV1::DeleteBackward,
            remove_second.result_selection_utf16,
            false,
        )
        .expect("remove first blank row");
    assert_eq!(
        remove_first.presentation_transition,
        DocumentEditPresentationTransitionV1::MergeParagraph
    );
    assert_eq!(remove_first.result_selection_utf16, 5);
    assert_eq!(source(&document), initial);
    document.close().expect("close paragraph gap");
}

#[test]
fn parsed_terminal_gap_remains_semantic_after_literal_extension() {
    let mut document = DocumentSession::begin("fff").expect("begin terminal paragraph");
    pump_ready(&mut document);

    let first = document
        .try_apply_edit_intent_v1(1, DocumentEditIntentV1::InsertParagraphBreak, 3, false)
        .expect("split terminal paragraph");
    assert_eq!(first.disposition, DocumentEditIntentDispositionV1::Applied);
    assert_eq!(first.result_selection_utf16, 5);
    assert_eq!(source(&document), "fff\n\n");
    pump_ready(&mut document);

    document
        .apply_edit(2, 5..5, "\n")
        .expect("literal terminal gap extension");
    assert_eq!(source(&document), "fff\n\n\n");
    pump_ready(&mut document);

    let extended = document
        .try_apply_edit_intent_v1(3, DocumentEditIntentV1::InsertParagraphBreak, 6, false)
        .expect("semantic terminal gap extension");
    assert_eq!(
        extended.disposition,
        DocumentEditIntentDispositionV1::Applied
    );
    assert_eq!(extended.result_selection_utf16, 7);
    assert_eq!(source(&document), "fff\n\n\n\n");
    document.close().expect("close terminal paragraph");
}

#[test]
fn generated_list_context_survives_immediate_literal_typing_and_return() {
    let initial = "- list item\n\n**sentinel**\n";
    let mut document = DocumentSession::begin(initial).expect("begin list sequence");
    pump_ready(&mut document);

    let continued = document
        .try_apply_edit_intent_v1(1, DocumentEditIntentV1::InsertParagraphBreak, 11, false)
        .expect("continue list");
    assert_eq!(
        continued.disposition,
        DocumentEditIntentDispositionV1::Applied
    );
    assert_eq!(source(&document), "- list item\n- \n\n**sentinel**\n");

    document
        .apply_edit(continued.result_revision, 14..14, "[")
        .expect("type into generated item");
    assert_eq!(source(&document), "- list item\n- [\n\n**sentinel**\n");

    let second = document
        .try_apply_edit_intent_v1(3, DocumentEditIntentV1::InsertParagraphBreak, 15, false)
        .expect("continue generated item before parser recertification");
    assert_eq!(second.disposition, DocumentEditIntentDispositionV1::Applied);
    assert_eq!(source(&document), "- list item\n- [\n- \n\n**sentinel**\n");
    document.close().expect("close list sequence");
}

#[test]
fn current_parser_context_supersedes_same_family_retained_list_geometry() {
    let initial = "1. outer\n   - inner\n\n**sentinel**\n";
    let mut document = DocumentSession::begin(initial).expect("begin nested list sequence");
    pump_ready(&mut document);

    let continued = document
        .try_apply_edit_intent_v1(1, DocumentEditIntentV1::InsertParagraphBreak, 19, false)
        .expect("continue nested list");
    assert_eq!(continued.result_selection_utf16, 25);
    document
        .apply_edit(continued.result_revision, 25..25, " ")
        .expect("insert blank list content");
    document
        .apply_edit(3, 26..26, "x")
        .expect("insert temporary list content");
    document
        .apply_edit(4, 26..27, "")
        .expect("remove temporary list content");
    pump_ready(&mut document);

    let outdented = document
        .try_apply_edit_intent_v1(5, DocumentEditIntentV1::InsertParagraphBreak, 26, false)
        .expect("outdent parser-certified blank nested item");
    assert_eq!(
        outdented.presentation_transition,
        DocumentEditPresentationTransitionV1::OutdentList
    );
    let splice = outdented
        .committed_splice
        .as_ref()
        .expect("committed nested-list outdent");
    assert_eq!(splice.base_utf16_range, 20..23);
    assert_eq!(splice.replacement, "");
    assert_eq!(outdented.result_selection_utf16, 23);
    assert_eq!(
        source(&document),
        "1. outer\n   - inner\n-  \n\n**sentinel**\n"
    );
    document.close().expect("close nested list sequence");
}

#[test]
fn terminated_list_context_survives_immediate_literal_typing_and_return() {
    let initial = "- list item\n\n**sentinel**\n";
    let mut document = DocumentSession::begin(initial).expect("begin list termination sequence");
    pump_ready(&mut document);

    let continued = document
        .try_apply_edit_intent_v1(1, DocumentEditIntentV1::InsertParagraphBreak, 11, false)
        .expect("continue list");
    let terminated = document
        .try_apply_edit_intent_v1(
            2,
            DocumentEditIntentV1::InsertParagraphBreak,
            continued.result_selection_utf16,
            false,
        )
        .expect("terminate empty generated list item");
    assert_eq!(
        terminated.disposition,
        DocumentEditIntentDispositionV1::Applied
    );
    assert_eq!(source(&document), "- list item\n\n\n\n**sentinel**\n");

    document
        .apply_edit(
            terminated.result_revision,
            terminated.result_selection_byte..terminated.result_selection_byte,
            "[",
        )
        .expect("type after terminated list");
    let split = document
        .try_apply_edit_intent_v1(
            4,
            DocumentEditIntentV1::InsertParagraphBreak,
            terminated.result_selection_utf16 + 1,
            false,
        )
        .expect("split literal paragraph before parser recertification");
    assert_eq!(split.disposition, DocumentEditIntentDispositionV1::Applied);
    assert_eq!(source(&document), "- list item\n\n[\n\n\n\n**sentinel**\n");
    document.close().expect("close list termination sequence");
}

#[test]
fn terminated_list_gap_backspaces_before_parser_recertification() {
    let initial = "- list item\n\n**sentinel**\n";
    let mut document = DocumentSession::begin(initial).expect("begin list gap sequence");
    pump_ready(&mut document);

    let continued = document
        .try_apply_edit_intent_v1(1, DocumentEditIntentV1::InsertParagraphBreak, 11, false)
        .expect("continue list");
    let terminated = document
        .try_apply_edit_intent_v1(
            2,
            DocumentEditIntentV1::InsertParagraphBreak,
            continued.result_selection_utf16,
            false,
        )
        .expect("terminate generated list item");
    assert_eq!(source(&document), "- list item\n\n\n\n**sentinel**\n");

    let removed = document
        .try_apply_edit_intent_v1(
            terminated.result_revision,
            DocumentEditIntentV1::DeleteBackward,
            terminated.result_selection_utf16,
            false,
        )
        .expect("remove exited list gap before recertification");
    assert_eq!(
        removed.disposition,
        DocumentEditIntentDispositionV1::Applied
    );
    assert_eq!(removed.result_selection_utf16, 12);
    assert_eq!(source(&document), "- list item\n\n\n**sentinel**\n");
    document.close().expect("close list gap sequence");
}

#[test]
fn settled_empty_list_backspace_carries_proven_plain_row_geometry() {
    let initial = "- list item\n- \n\n**sentinel**\n";
    let mut document = DocumentSession::begin(initial).expect("begin settled empty list");
    pump_ready(&mut document);

    let receipt = document
        .try_apply_edit_intent_v1(1, DocumentEditIntentV1::DeleteBackward, 14, false)
        .expect("lift settled empty list item");
    assert_eq!(
        receipt.disposition,
        DocumentEditIntentDispositionV1::Applied
    );
    assert_eq!(
        receipt.presentation_transition,
        DocumentEditPresentationTransitionV1::LiftList
    );
    assert!(receipt.presentation_proven);
    let splice = receipt
        .committed_splice
        .as_ref()
        .expect("committed list lift");
    assert_eq!(splice.base_utf16_range, 12..14);
    assert_eq!(splice.replacement, "\n");
    assert_eq!(splice.result_utf16_range, 12..13);
    assert_eq!(receipt.result_selection_utf16, 13);
    assert_eq!(source(&document), "- list item\n\n\n\n**sentinel**\n");
    document.close().expect("close settled empty list");
}

#[test]
fn paragraph_break_at_a_parser_owned_list_marker_boundary_reuses_its_padding() {
    let initial = "| a\n* | b |\n| --- | --- |\n| c | d |\n\n**sentinel**\n";
    let mut document = DocumentSession::begin(initial).expect("begin marker-boundary list");
    pump_ready(&mut document);

    let receipt = document
        .try_apply_edit_intent_v1(1, DocumentEditIntentV1::InsertParagraphBreak, 5, false)
        .expect("split at marker boundary");
    assert_eq!(
        receipt.disposition,
        DocumentEditIntentDispositionV1::Applied
    );
    assert_eq!(
        receipt.presentation_transition,
        DocumentEditPresentationTransitionV1::ContinueList
    );
    assert_eq!(receipt.result_selection_utf16, 8);
    assert_eq!(
        source(&document),
        "| a\n* \n* | b |\n| --- | --- |\n| c | d |\n\n**sentinel**\n"
    );
    document.close().expect("close marker-boundary list");
}

#[test]
fn collapsed_terminated_list_gap_retains_lazy_continuation_semantics() {
    let initial = "- list item\n\n**sentinel**\n";
    let mut document = DocumentSession::begin(initial).expect("begin lazy list sequence");
    pump_ready(&mut document);

    let continued = document
        .try_apply_edit_intent_v1(1, DocumentEditIntentV1::InsertParagraphBreak, 11, false)
        .expect("continue list");
    let terminated = document
        .try_apply_edit_intent_v1(
            2,
            DocumentEditIntentV1::InsertParagraphBreak,
            continued.result_selection_utf16,
            false,
        )
        .expect("terminate generated list item");
    let removed = document
        .try_apply_edit_intent_v1(
            terminated.result_revision,
            DocumentEditIntentV1::DeleteBackward,
            terminated.result_selection_utf16,
            false,
        )
        .expect("collapse one exited list gap");
    document
        .apply_edit(
            removed.result_revision,
            removed.result_selection_byte..removed.result_selection_byte,
            ".",
        )
        .expect("type a lazy continuation before parser recertification");

    let split = document
        .try_apply_edit_intent_v1(
            5,
            DocumentEditIntentV1::InsertParagraphBreak,
            removed.result_selection_utf16 + 1,
            false,
        )
        .expect("continue the parser-owned list before recertification");
    assert_eq!(split.disposition, DocumentEditIntentDispositionV1::Applied);
    assert_eq!(
        split.presentation_transition,
        DocumentEditPresentationTransitionV1::ContinueList
    );
    assert_eq!(source(&document), "- list item\n.\n- \n\n**sentinel**\n");
    document.close().expect("close lazy list sequence");
}

#[test]
fn initial_pending_exact_context_distinguishes_later_list_items() {
    let mut document = DocumentSession::begin("- one\n- two\n").expect("begin pending list");
    let receipt = document
        .try_apply_edit_intent_v1(1, DocumentEditIntentV1::DeleteBackward, 8, false)
        .expect("lift later pending item");
    assert_eq!(
        receipt.disposition,
        DocumentEditIntentDispositionV1::Applied
    );
    assert_eq!(source(&document), "- one\n\ntwo\n");
    document.close().expect("close pending list");
}

#[test]
fn initial_pending_exact_context_preserves_task_semantics() {
    let mut document = DocumentSession::begin("- [x] task\n").expect("begin pending task");
    let receipt = document
        .try_apply_edit_intent_v1(1, DocumentEditIntentV1::InsertParagraphBreak, 10, false)
        .expect("continue pending task");
    assert_eq!(
        receipt.disposition,
        DocumentEditIntentDispositionV1::Applied
    );
    assert_eq!(source(&document), "- [x] task\n- [ ] \n");
    document.close().expect("close pending task");
}

#[test]
fn parser_pending_quote_continues_then_exits_without_pumping() {
    let mut document = DocumentSession::begin("> alpha").expect("begin pending quote");
    let continued = document
        .try_apply_edit_intent_v1(1, DocumentEditIntentV1::InsertParagraphBreak, 7, false)
        .expect("continue pending quote");
    assert_eq!(
        continued.disposition,
        DocumentEditIntentDispositionV1::Applied
    );
    assert_eq!(source(&document), "> alpha\n> ");

    let exited = document
        .try_apply_edit_intent_v1(
            2,
            DocumentEditIntentV1::InsertParagraphBreak,
            continued.result_selection_utf16,
            false,
        )
        .expect("exit pending quote");
    assert_eq!(exited.disposition, DocumentEditIntentDispositionV1::Applied);
    assert_eq!(source(&document), "> alpha\n\n");
    document.close().expect("close pending quote");
}

#[test]
fn parser_pending_nested_quote_outdents_one_level_per_command() {
    let mut document = DocumentSession::begin("> > > ").expect("begin nested quote");
    pump_ready(&mut document);

    let first = document
        .try_apply_edit_intent_v1(1, DocumentEditIntentV1::InsertParagraphBreak, 6, false)
        .expect("first quote outdent");
    assert_eq!(
        first.presentation_transition,
        DocumentEditPresentationTransitionV1::OutdentBlockQuote,
    );
    assert_eq!(source(&document), "> > ");
    assert_eq!(first.result_selection_utf16, 4);

    let second = document
        .try_apply_edit_intent_v1(2, DocumentEditIntentV1::InsertParagraphBreak, 4, false)
        .expect("second quote outdent while parser pending");
    assert_eq!(
        second.presentation_transition,
        DocumentEditPresentationTransitionV1::OutdentBlockQuote,
    );
    assert_eq!(source(&document), "> ");
    assert_eq!(second.result_selection_utf16, 2);

    let exit = document
        .try_apply_edit_intent_v1(3, DocumentEditIntentV1::InsertParagraphBreak, 2, false)
        .expect("exit outer quote while parser pending");
    assert_eq!(
        exit.presentation_transition,
        DocumentEditPresentationTransitionV1::ExitBlockQuote,
    );
    assert_eq!(source(&document), "\n");
    document.close().expect("close nested quote");
}

#[test]
fn parser_pending_indented_code_continues_without_pumping() {
    let mut document = DocumentSession::begin("    code").expect("begin indented code sequence");
    pump_ready(&mut document);
    let first = document
        .try_apply_edit_intent_v1(1, DocumentEditIntentV1::InsertParagraphBreak, 8, false)
        .expect("first indented code Return");
    assert_eq!(
        first.presentation_transition,
        DocumentEditPresentationTransitionV1::ContinueIndentedCode
    );
    assert_eq!(source(&document), "    code\n    ");

    document
        .apply_edit(
            2,
            first.result_selection_utf16..first.result_selection_utf16,
            "next",
        )
        .expect("type into pending indented code line");
    let second = document
        .try_apply_edit_intent_v1(
            3,
            DocumentEditIntentV1::InsertParagraphBreak,
            first.result_selection_utf16 + 4,
            false,
        )
        .expect("second pending indented code Return");
    assert_eq!(
        second.presentation_transition,
        DocumentEditPresentationTransitionV1::ContinueIndentedCode
    );
    assert_eq!(source(&document), "    code\n    next\n    ");
    document.close().expect("close indented code sequence");
}

#[test]
fn parser_pending_depth_two_list_continues_then_outdents_without_pumping() {
    let mut document =
        DocumentSession::begin("- parent\n  - child").expect("begin nested sequence");
    pump_ready(&mut document);
    let continued = document
        .try_apply_edit_intent_v1(1, DocumentEditIntentV1::InsertParagraphBreak, 18, false)
        .expect("continue nested item");
    assert_eq!(
        continued.presentation_transition,
        DocumentEditPresentationTransitionV1::ContinueList
    );
    assert_eq!(source(&document), "- parent\n  - child\n  - ");
    assert_ne!(document.phase(), DocumentSessionPhase::Ready);

    let outdented = document
        .try_apply_edit_intent_v1(2, DocumentEditIntentV1::InsertParagraphBreak, 23, false)
        .expect("outdent generated empty item");
    assert_eq!(
        outdented.presentation_transition,
        DocumentEditPresentationTransitionV1::OutdentList
    );
    assert_eq!(outdented.result_selection_utf16, 21);
    assert_eq!(source(&document), "- parent\n  - child\n- ");
    document.close().expect("close nested sequence");
}

#[test]
fn parser_pending_depth_three_list_outdents_one_level_per_command() {
    let mut document = DocumentSession::begin("- root\n  - child\n    - leaf")
        .expect("begin depth-three sequence");
    pump_ready(&mut document);
    let continued = document
        .try_apply_edit_intent_v1(1, DocumentEditIntentV1::InsertParagraphBreak, 27, false)
        .expect("continue depth-three item");
    assert_eq!(source(&document), "- root\n  - child\n    - leaf\n    - ");

    let depth_two = document
        .try_apply_edit_intent_v1(
            2,
            DocumentEditIntentV1::InsertParagraphBreak,
            continued.result_selection_utf16,
            false,
        )
        .expect("outdent to depth two");
    assert_eq!(depth_two.result_selection_utf16, 32);
    assert_eq!(source(&document), "- root\n  - child\n    - leaf\n  - ");

    let depth_one = document
        .try_apply_edit_intent_v1(
            3,
            DocumentEditIntentV1::InsertParagraphBreak,
            depth_two.result_selection_utf16,
            false,
        )
        .expect("outdent to depth one");
    assert_eq!(depth_one.result_selection_utf16, 30);
    assert_eq!(source(&document), "- root\n  - child\n    - leaf\n- ");
    document.close().expect("close depth-three sequence");
}

#[test]
fn terminal_newline_depth_three_list_continues() {
    let mut document = DocumentSession::begin("- root\n  - child\n    - leaf\n")
        .expect("begin terminated depth-three sequence");
    pump_ready(&mut document);
    let continued = document
        .try_apply_edit_intent_v1(1, DocumentEditIntentV1::InsertParagraphBreak, 27, false)
        .expect("continue terminated depth-three item");
    assert_eq!(
        continued.presentation_transition,
        DocumentEditPresentationTransitionV1::ContinueList
    );
    assert_eq!(source(&document), "- root\n  - child\n    - leaf\n    - \n");
    document.close().expect("close terminated sequence");
}

#[test]
fn nonuniform_nested_list_geometry_outdents_by_parent_width() {
    let initial = "10. root\n    - child\n";
    let mut document = DocumentSession::begin(initial).expect("begin nonuniform nested List");
    pump_ready(&mut document);
    let receipt = document
        .try_apply_edit_intent_v1(1, DocumentEditIntentV1::DeleteBackward, 15, false)
        .expect("resolve nonuniform nested List");
    assert_eq!(
        receipt.presentation_transition,
        DocumentEditPresentationTransitionV1::OutdentList,
        "{receipt:#?}"
    );
    assert_eq!(receipt.result_selection_utf16, 11);
    assert_eq!(document.revision(), 2);
    assert_eq!(source(&document), "10. root\n- child\n");
    document.close().expect("close nonuniform nested List");
}

#[test]
fn nonuniform_nested_list_continues_then_outdents_by_parent_width() {
    let mut document =
        DocumentSession::begin("10. root\n    - child").expect("begin nonuniform nested sequence");
    pump_ready(&mut document);
    let continued = document
        .try_apply_edit_intent_v1(1, DocumentEditIntentV1::InsertParagraphBreak, 20, false)
        .expect("continue nonuniform nested item");
    assert_eq!(continued.result_selection_utf16, 27);
    assert_eq!(source(&document), "10. root\n    - child\n    - ");

    let outdented = document
        .try_apply_edit_intent_v1(
            2,
            DocumentEditIntentV1::InsertParagraphBreak,
            continued.result_selection_utf16,
            false,
        )
        .expect("outdent nonuniform generated item");
    assert_eq!(
        outdented.presentation_transition,
        DocumentEditPresentationTransitionV1::OutdentList
    );
    assert_eq!(outdented.result_selection_utf16, 23);
    assert_eq!(source(&document), "10. root\n    - child\n- ");
    document.close().expect("close nonuniform nested sequence");
}

#[test]
fn nonuniform_outdent_preserves_the_current_item_marker_offset() {
    let mut document = DocumentSession::begin("10. root\n     - child\n")
        .expect("begin offset nonuniform nested List");
    pump_ready(&mut document);
    let receipt = document
        .try_apply_edit_intent_v1(1, DocumentEditIntentV1::DeleteBackward, 16, false)
        .expect("outdent offset nonuniform nested item");
    assert_eq!(
        receipt.presentation_transition,
        DocumentEditPresentationTransitionV1::OutdentList,
        "{receipt:#?}"
    );
    assert_eq!(receipt.result_selection_utf16, 12);
    assert_eq!(source(&document), "10. root\n - child\n");
    document
        .close()
        .expect("close offset nonuniform nested List");
}

#[test]
fn task_toggle_targets_a_certified_row_without_moving_the_selection() {
    let initial = "- [ ] task\n\nselection stays here\n";
    let target = initial.find("task").expect("task content");
    let selection = initial.find("stays").expect("independent selection");
    let mut document = DocumentSession::begin(initial).expect("begin task fixture");
    pump_ready(&mut document);

    let checked = document
        .try_apply_edit_intent_v1_at_bytes(
            1,
            DocumentEditIntentV1::ToggleTaskChecked,
            selection,
            target,
            false,
        )
        .expect("check task");
    assert_eq!(
        checked.disposition,
        DocumentEditIntentDispositionV1::Applied
    );
    assert_eq!(
        checked.presentation_transition,
        DocumentEditPresentationTransitionV1::ToggleTaskChecked
    );
    assert_eq!(checked.result_selection_byte, selection);
    assert_eq!(checked.result_selection_utf16, selection);
    let splice = checked.committed_splice.expect("committed check splice");
    assert_eq!(splice.base_byte_range, 3..4);
    assert_eq!(splice.base_utf16_range, 3..4);
    assert_eq!(splice.replacement, "x");
    assert_eq!(source(&document), "- [x] task\n\nselection stays here\n");

    // The carried edit context makes a repeated action deterministic even
    // before the incremental parser has recertified the row.
    let unchecked = document
        .try_apply_edit_intent_v1_at_bytes(
            2,
            DocumentEditIntentV1::ToggleTaskChecked,
            selection,
            target,
            false,
        )
        .expect("uncheck task while parser is pending");
    assert_eq!(unchecked.result_selection_byte, selection);
    assert_eq!(source(&document), initial);
    document.close().expect("close task fixture");
}

#[test]
fn task_toggle_is_fail_closed_for_non_tasks_and_composition() {
    let initial = "- item\n";
    let mut document = DocumentSession::begin(initial).expect("begin plain list");
    pump_ready(&mut document);
    let not_a_task = document
        .try_apply_edit_intent_v1_at_bytes(1, DocumentEditIntentV1::ToggleTaskChecked, 6, 2, false)
        .expect("reject plain list action");
    assert_eq!(
        not_a_task.disposition,
        DocumentEditIntentDispositionV1::NotApplicable
    );
    assert_eq!(source(&document), initial);

    let mut task = DocumentSession::begin("- [X] task\n").expect("begin checked task");
    pump_ready(&mut task);
    let composing = task
        .try_apply_edit_intent_v1_at_bytes(1, DocumentEditIntentV1::ToggleTaskChecked, 10, 6, true)
        .expect("composition guard");
    assert_eq!(
        composing.disposition,
        DocumentEditIntentDispositionV1::NotApplicable
    );
    assert_eq!(source(&task), "- [X] task\n");
    document.close().expect("close plain list");
    task.close().expect("close checked task");
}

#[test]
fn resolver_cost_is_size_independent_for_large_and_giant_lines() {
    let ordinary_tail = "paragraph padding for a large ordinary document.\n\n".repeat(24_000);
    let ordinary_10m_tail = "paragraph padding for a large ordinary document.\n\n".repeat(200_000);
    let giant_tail = "x".repeat(1024 * 1024);
    for (shape, initial, caret) in [
        (
            "ordinary-1MiB",
            format!("target paragraph\n\n{ordinary_tail}"),
            "target paragraph".len(),
        ),
        (
            "giant-line-1MiB",
            format!("target {giant_tail}\n"),
            7 + giant_tail.len() / 2,
        ),
        (
            "ordinary-10MiB",
            format!("target paragraph\n\n{ordinary_10m_tail}"),
            "target paragraph".len(),
        ),
    ] {
        let mut document = DocumentSession::begin(&initial).expect("begin large fixture");
        pump_ready(&mut document);
        let started = Instant::now();
        let receipt = document
            .try_apply_edit_intent_v1(1, DocumentEditIntentV1::InsertParagraphBreak, caret, false)
            .unwrap_or_else(|error| panic!("{shape}: {error:?}"));
        let elapsed = started.elapsed();
        eprintln!("{shape} semantic resolve+commit: {elapsed:?}");
        assert_eq!(
            receipt.disposition,
            DocumentEditIntentDispositionV1::Applied,
            "{shape}"
        );
        if !cfg!(debug_assertions) {
            assert!(
                elapsed < Duration::from_millis(20),
                "{shape} semantic call took {elapsed:?}"
            );
        }
        document.close().expect("close large fixture");
    }
}
