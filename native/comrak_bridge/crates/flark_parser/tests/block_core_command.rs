use flark_parser::block_core::{
    BlockCommand, BlockKind, BulletMarker, ChildSequenceFold, ClosedChild, CoveragePart,
    FenceCharacter, FencedCodeBoundary, FencedCodeCloseFacts, FencedCodeFacts, FinalFacts,
    HeadingFacts, HeadingStyle, HtmlBlockFacts, HtmlBlockType, ItemFacts, LineEnding,
    LineSourcePosition, LineSourceRange, ListDelimiter, ListFacts, ListStyle, LogicalAction,
    ParagraphOutcome, PartialTab, SetextHeadingLevel, SourceMetric, StackOwner,
    TerminatorResolution,
};

fn position(byte: u64, utf16: u64) -> LineSourcePosition {
    LineSourcePosition::new(byte, utf16)
}

fn source_range(
    start_byte: u64,
    end_byte: u64,
    start_utf16: u64,
    end_utf16: u64,
) -> LineSourceRange {
    LineSourceRange::new(
        position(start_byte, start_utf16),
        position(end_byte, end_utf16),
    )
    .expect("valid exact source range")
}

#[test]
fn exact_source_values_keep_byte_and_utf16_axes_together() {
    let emoji = source_range(7, 11, 7, 9);
    assert_eq!(emoji.start(), position(7, 7));
    assert_eq!(emoji.end(), position(11, 9));
    assert_eq!(emoji.metric(), SourceMetric::new(4, 2).unwrap());

    assert!(LineSourceRange::new(position(1, 1), position(1, 1)).is_none());
    assert!(LineSourceRange::new(position(1, 1), position(2, 1)).is_none());
    assert!(LineSourceRange::new(position(1, 1), position(1, 2)).is_none());
    assert!(LineSourceRange::new(position(4, 2), position(3, 3)).is_none());
    assert!(LineSourceRange::new(position(1, 1), position(2, 3)).is_none());

    assert_eq!(SourceMetric::new(0, 0).unwrap(), SourceMetric::default());
    assert!(SourceMetric::new(0, 1).is_none());
    assert!(SourceMetric::new(1, 0).is_none());
    assert!(SourceMetric::new(1, 2).is_none());
    assert_eq!(
        SourceMetric::new(4, 2)
            .unwrap()
            .checked_add(SourceMetric::new(2, 1).unwrap()),
        SourceMetric::new(6, 3),
    );
    assert!(SourceMetric::new(u64::MAX, 1)
        .unwrap()
        .checked_add(SourceMetric::new(1, 1).unwrap())
        .is_none());
}

#[test]
fn typed_open_and_close_facts_exclude_invalid_markdown_shapes() {
    let atx = HeadingFacts::new(6, HeadingStyle::Atx).unwrap();
    let setext = HeadingFacts::new(2, HeadingStyle::Setext).unwrap();
    assert_eq!(atx.level(), 6);
    assert_eq!(setext.style(), HeadingStyle::Setext);
    assert!(HeadingFacts::new(0, HeadingStyle::Atx).is_none());
    assert!(HeadingFacts::new(7, HeadingStyle::Atx).is_none());
    assert!(HeadingFacts::new(3, HeadingStyle::Setext).is_none());

    let fence = FencedCodeFacts::new(FenceCharacter::Backtick, 7, 3).unwrap();
    assert_eq!(fence.fence().marker(), b'`');
    assert_eq!(fence.minimum_closing_length(), 7);
    assert_eq!(fence.fence_offset_columns(), 3);
    assert!(FencedCodeFacts::new(FenceCharacter::Tilde, 2, 0).is_none());
    assert!(FencedCodeFacts::new(FenceCharacter::Tilde, 3, 4).is_none());

    let bullet = ListFacts::bullet(BulletMarker::Asterisk);
    assert_eq!(
        bullet.style(),
        ListStyle::Bullet {
            marker: BulletMarker::Asterisk
        }
    );
    let ordered = ListFacts::ordered(42, ListDelimiter::Parenthesis).unwrap();
    assert_eq!(
        ordered.style(),
        ListStyle::Ordered {
            start: 42,
            delimiter: ListDelimiter::Parenthesis
        }
    );
    assert!(ListFacts::ordered(ListFacts::MAX_ORDERED_START + 1, ListDelimiter::Period).is_none());

    let item = ItemFacts::new(3, 14).unwrap();
    assert_eq!(item.effective_content_indent(), 17);
    assert!(ItemFacts::new(4, 1).is_none());
    assert!(ItemFacts::new(0, 0).is_none());
    assert!(ItemFacts::new(0, 1).is_none());
    assert!(ItemFacts::new(0, 15).is_none());

    assert_eq!(
        ParagraphOutcome::setext_heading(1),
        Some(ParagraphOutcome::SetextHeading {
            level: SetextHeadingLevel::One
        })
    );
    assert_eq!(SetextHeadingLevel::Two.get(), 2);
    assert!(ParagraphOutcome::setext_heading(3).is_none());

    for value in 1..=7 {
        let block_type = HtmlBlockType::new(value).expect("CommonMark HTML block family");
        assert_eq!(block_type.get(), value);
        assert_eq!(HtmlBlockFacts::new(block_type).block_type(), block_type);
    }
    assert!(HtmlBlockType::new(0).is_none());
    assert!(HtmlBlockType::new(8).is_none());

    let remaining_commonmark_blocks = [
        BlockKind::IndentedCode,
        BlockKind::HtmlBlock(HtmlBlockFacts::new(HtmlBlockType::Seven)),
        BlockKind::ThematicBreak,
    ];
    assert_eq!(remaining_commonmark_blocks.len(), 3);
}

#[test]
fn one_generic_stack_protocol_expresses_nested_container_commands() {
    let marker = source_range(0, 2, 0, 2);
    let quote = source_range(2, 4, 2, 4);
    let content = source_range(4, 8, 4, 6);
    let newline = source_range(8, 10, 6, 8);
    let fence = FencedCodeFacts::new(FenceCharacter::Tilde, 3, 2).unwrap();

    let commands = [
        BlockCommand::Enter {
            kind: BlockKind::Document,
        },
        BlockCommand::Enter {
            kind: BlockKind::List(ListFacts::bullet(BulletMarker::Hyphen)),
        },
        BlockCommand::Enter {
            kind: BlockKind::Item(ItemFacts::new(0, 2).unwrap()),
        },
        BlockCommand::Coverage {
            owner: StackOwner::TOP,
            part: CoveragePart::BlockMarker,
            source: marker,
            logical: LogicalAction::None,
        },
        BlockCommand::Enter {
            kind: BlockKind::BlockQuote,
        },
        BlockCommand::Coverage {
            owner: StackOwner::TOP,
            part: CoveragePart::ContainerMarker,
            source: quote,
            logical: LogicalAction::None,
        },
        BlockCommand::Enter {
            kind: BlockKind::FencedCode(fence),
        },
        BlockCommand::Coverage {
            owner: StackOwner::TOP,
            part: CoveragePart::Content,
            source: content,
            logical: LogicalAction::CanonicalText,
        },
        BlockCommand::StageTerminator {
            source: newline,
            ending: LineEnding::CrLf,
        },
        BlockCommand::ResolveTerminator {
            resolution: TerminatorResolution::ContinueCanonicalNewline,
        },
        BlockCommand::MarkFencedCodeBoundary {
            boundary: FencedCodeBoundary::LiteralStart,
        },
        BlockCommand::Close {
            kind: BlockKind::FencedCode(fence),
            final_facts: FinalFacts::FencedCode(FencedCodeCloseFacts::new(true)),
            last_line_blank: false,
            child: ClosedChild::default(),
        },
        BlockCommand::Close {
            kind: BlockKind::BlockQuote,
            final_facts: FinalFacts::None,
            last_line_blank: false,
            child: ClosedChild::default(),
        },
        BlockCommand::Close {
            kind: BlockKind::Item(ItemFacts::new(0, 2).unwrap()),
            final_facts: FinalFacts::None,
            last_line_blank: false,
            child: ClosedChild::default(),
        },
        BlockCommand::Close {
            kind: BlockKind::List(ListFacts::bullet(BulletMarker::Hyphen)),
            final_facts: FinalFacts::List { tight: true },
            last_line_blank: false,
            child: ClosedChild::default(),
        },
        BlockCommand::Close {
            kind: BlockKind::Document,
            final_facts: FinalFacts::None,
            last_line_blank: false,
            child: ClosedChild::default(),
        },
        BlockCommand::FinishLine {
            physical: SourceMetric::new(10, 8).unwrap(),
        },
        BlockCommand::FinishDocument,
    ];

    assert_eq!(commands.len(), 18);
    assert!(matches!(
        commands[7],
        BlockCommand::Coverage {
            owner: StackOwner::TOP,
            source,
            ..
        } if source.metric() == SourceMetric::new(4, 2).unwrap()
    ));
    assert!(matches!(
        commands[14],
        BlockCommand::Close {
            kind: BlockKind::List(_),
            final_facts: FinalFacts::List { tight: true },
            ..
        }
    ));

    let tab = PartialTab::new(StackOwner::ancestor(2), 3).unwrap();
    assert_eq!(tab.logical_target(), StackOwner::ancestor(2));
    assert_eq!(tab.remaining_spaces(), 3);
    assert!(PartialTab::new(StackOwner::TOP, 0).is_none());
    assert!(PartialTab::new(StackOwner::TOP, 4).is_none());
}

fn fold(children: &[ClosedChild]) -> ChildSequenceFold {
    let mut fold = ChildSequenceFold::default();
    for child in children {
        fold.push(*child);
    }
    fold
}

#[test]
fn child_fold_composition_is_associative_and_matches_direct_folding() {
    let children = [
        ClosedChild::new(false, false, false),
        ClosedChild::new(true, false, false),
        ClosedChild::new(false, true, false),
        ClosedChild::new(false, false, true),
        ClosedChild::new(true, true, false),
        ClosedChild::new(true, false, true),
        ClosedChild::new(false, true, true),
        ClosedChild::new(true, true, true),
    ];

    for first in children {
        for second in children {
            for third in children {
                let direct = fold(&[first, second, third]);
                let left = fold(&[first])
                    .followed_by(fold(&[second]))
                    .followed_by(fold(&[third]));
                let right = fold(&[first]).followed_by(fold(&[second]).followed_by(fold(&[third])));
                assert_eq!(left, direct);
                assert_eq!(right, direct);
            }
        }
    }

    assert_eq!(
        ChildSequenceFold::default().followed_by(fold(&children)),
        fold(&children)
    );
    assert_eq!(
        fold(&children).followed_by(ChildSequenceFold::default()),
        fold(&children)
    );
}

#[test]
fn last_child_blank_propagation_preserves_list_tightness_semantics() {
    let mut empty = ChildSequenceFold::default();
    empty.mark_last_child_line_blank();
    assert!(!empty.had_child());

    let mut one = fold(&[ClosedChild::default()]);
    assert!(one.list_is_tight());
    one.mark_last_child_line_blank();
    assert!(one.last_child_ends_blank());
    assert!(one.last_item_loose_if_nonlast());
    assert!(one.list_is_tight());

    one.push(ClosedChild::default());
    assert!(one.list_loose_before_last());
    assert!(!one.list_is_tight());
}

#[test]
fn block_core_is_a_scalar_only_semantics_boundary() {
    const MANIFEST: &str = include_str!("../Cargo.toml");
    const MODULE: &str = include_str!("../src/block_core/mod.rs");
    const COMMAND: &str = include_str!("../src/block_core/command.rs");
    const CHILD_FOLD: &str = include_str!("../src/block_core/child_fold.rs");

    for source in [MODULE, COMMAND, CHILD_FOLD] {
        assert!(source.contains("SPDX-License-Identifier: BSD-2-Clause"));
        for forbidden in [
            "flark_engine",
            "comrak::",
            "parse_document(",
            "markdown_to_html(",
            "NodeId",
            "BlockTree",
            "BlockEvent",
            "SourceDocument",
            "StructuralEvent",
            "TreeMaterializer",
            "ValueBlockParser",
            "String",
            "Box<",
            "Arc<",
            "Vec<",
            "VecDeque",
            "HashMap",
            "HashSet",
        ] {
            assert!(
                !source.contains(forbidden),
                "block_core contains forbidden dependency or proof type: {forbidden}"
            );
        }
    }

    fn assert_copy<T: Copy>() {}
    assert_copy::<BlockCommand>();
    assert_copy::<ChildSequenceFold>();
    assert!(!std::mem::needs_drop::<BlockCommand>());
    assert!(
        MANIFEST.contains("license = \"MIT AND BSD-2-Clause\""),
        "the parser package must retain both Flark and correspondent Comrak terms"
    );
}
