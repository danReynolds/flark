use std::time::{Duration, Instant};

use flark_runtime::{
    DocumentEditIntentDispositionV1, DocumentEditIntentV1, DocumentEditPresentationTransitionV1,
    DocumentSession, DocumentSessionPhase,
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
fn complex_context_fails_closed_and_composition_never_mutates() {
    for initial in [
        "> > nested\n",
        "> # child heading\n",
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
