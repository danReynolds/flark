use std::sync::Arc;

use flark_integrated_parser_slice::frontier::{
    LexerStatus, SegmentedLeaf, SegmentedLeafBuilder, SharedLexer, MAX_LEXER_POLL_WORK,
};
use flark_integrated_parser_slice::grammar::{
    Alignment, FallbackReason, GrammarClassification, GrammarJob, GrammarOutput, GrammarRecord,
    GrammarStatus, MAX_ATOMIC_REGION_INDEX_UNITS, MAX_GRAMMAR_LEAF_BYTES,
    MAX_INLINE_EVENTS_PER_REGION,
};
use flark_integrated_parser_slice::source::PersistentSource;
use pulldown_cmark::{Alignment as PulldownAlignment, Event, Options, Parser, Tag, TagEnd};

fn leaf_for(text: &str) -> SegmentedLeaf {
    let source = Arc::new(PersistentSource::from_text(text));
    let mut builder = SegmentedLeafBuilder::new(source.clone());
    if !text.is_empty() {
        builder.push_source(0..text.len()).unwrap();
    }
    builder.finish()
}

fn lex_to_ready(leaf: &SegmentedLeaf) -> SharedLexer {
    let mut lexer = SharedLexer::new(leaf);
    loop {
        if lexer.poll(MAX_LEXER_POLL_WORK).status == LexerStatus::Ready {
            return lexer;
        }
    }
}

fn parse_with_fuel(text: &str, fuel: usize) -> GrammarOutput {
    let leaf = leaf_for(text);
    parse_leaf_with_fuel(&leaf, fuel)
}

fn parse_leaf_with_fuel(leaf: &SegmentedLeaf, fuel: usize) -> GrammarOutput {
    let mut grammar = {
        let lexer = lex_to_ready(leaf);
        let consumers = lexer.consumers().unwrap();
        GrammarJob::new(&consumers).unwrap()
    };
    loop {
        let poll = grammar.poll(fuel);
        assert!(poll.work <= fuel);
        if poll.status == GrammarStatus::Ready {
            return grammar.result().unwrap().clone();
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Token {
    ParagraphStart,
    ParagraphEnd,
    Text(String),
    SoftBreak,
    Code(String),
    EmphasisStart,
    EmphasisEnd,
    StrongStart,
    StrongEnd,
    TableStart,
    TableEnd,
    TableHeadStart,
    TableHeadEnd,
    TableRowStart,
    TableRowEnd,
    TableCellStart,
    TableCellEnd,
}

fn push_token(tokens: &mut Vec<Token>, token: Token) {
    if let Token::Text(text) = token {
        if text.is_empty() {
            return;
        }
        if let Some(Token::Text(previous)) = tokens.last_mut() {
            previous.push_str(&text);
        } else {
            tokens.push(Token::Text(text));
        }
    } else {
        tokens.push(token);
    }
}

fn normalized_code(raw: &str, table_cell: bool) -> String {
    let table_unescaped = if table_cell {
        raw.replace("\\|", "|")
    } else {
        raw.to_owned()
    };
    let normalized = table_unescaped.replace("\r\n", "\n").replace('\r', "\n");
    let mut normalized = normalized.replace('\n', " ");
    if normalized.starts_with(' ')
        && normalized.ends_with(' ')
        && normalized.bytes().any(|byte| byte != b' ')
    {
        normalized.remove(0);
        normalized.pop();
    }
    normalized
}

fn push_raw_text(tokens: &mut Vec<Token>, raw: &str, terminal: bool) {
    let bytes = raw.as_bytes();
    let mut start = 0;
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'\r' || bytes[index] == b'\n' {
            push_token(&mut *tokens, Token::Text(raw[start..index].to_owned()));
            let line_end_len = usize::from(bytes[index] == b'\r')
                + usize::from(bytes.get(index + 1) == Some(&b'\n'));
            index += line_end_len.max(1);
            if !(terminal && index == bytes.len()) {
                push_token(&mut *tokens, Token::SoftBreak);
            }
            start = index;
        } else {
            index += 1;
        }
    }
    push_token(&mut *tokens, Token::Text(raw[start..].to_owned()));
}

fn own_tokens(text: &str, output: &GrammarOutput) -> Vec<Token> {
    let mut tokens = Vec::new();
    for record in output.records.records() {
        match record {
            GrammarRecord::ParagraphStart { .. } => push_token(&mut tokens, Token::ParagraphStart),
            GrammarRecord::ParagraphEnd => push_token(&mut tokens, Token::ParagraphEnd),
            GrammarRecord::Text { start, end } => {
                push_raw_text(&mut tokens, &text[start..end], end == text.len());
            }
            GrammarRecord::Escaped { byte, .. } => {
                push_token(&mut tokens, Token::Text(char::from(byte).to_string()));
            }
            GrammarRecord::Code {
                content_start,
                content_end,
                table_cell,
                ..
            } => push_token(
                &mut tokens,
                Token::Code(normalized_code(
                    &text[content_start..content_end],
                    table_cell,
                )),
            ),
            GrammarRecord::EmphasisStart { .. } => {
                push_token(&mut tokens, Token::EmphasisStart);
            }
            GrammarRecord::EmphasisEnd { .. } => push_token(&mut tokens, Token::EmphasisEnd),
            GrammarRecord::StrongStart { .. } => push_token(&mut tokens, Token::StrongStart),
            GrammarRecord::StrongEnd { .. } => push_token(&mut tokens, Token::StrongEnd),
            GrammarRecord::TableStart { .. } => push_token(&mut tokens, Token::TableStart),
            GrammarRecord::TableEnd => push_token(&mut tokens, Token::TableEnd),
            GrammarRecord::TableHeadStart => push_token(&mut tokens, Token::TableHeadStart),
            GrammarRecord::TableHeadEnd => push_token(&mut tokens, Token::TableHeadEnd),
            GrammarRecord::TableRowStart => push_token(&mut tokens, Token::TableRowStart),
            GrammarRecord::TableRowEnd => push_token(&mut tokens, Token::TableRowEnd),
            GrammarRecord::TableCellStart { .. } => {
                push_token(&mut tokens, Token::TableCellStart);
            }
            GrammarRecord::TableCellEnd => push_token(&mut tokens, Token::TableCellEnd),
            GrammarRecord::LiteralFallback { .. } => {}
        }
    }
    tokens
}

fn pulldown_tokens(text: &str) -> Vec<Token> {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    let mut tokens = Vec::new();
    for event in Parser::new_ext(text, options) {
        let token = match event {
            Event::Start(Tag::Paragraph) => Some(Token::ParagraphStart),
            Event::End(TagEnd::Paragraph) => Some(Token::ParagraphEnd),
            Event::Text(text) => Some(Token::Text(text.into_string())),
            Event::Code(code) => Some(Token::Code(code.into_string())),
            Event::Start(Tag::Emphasis) => Some(Token::EmphasisStart),
            Event::End(TagEnd::Emphasis) => Some(Token::EmphasisEnd),
            Event::Start(Tag::Strong) => Some(Token::StrongStart),
            Event::End(TagEnd::Strong) => Some(Token::StrongEnd),
            Event::Start(Tag::Table(_)) => Some(Token::TableStart),
            Event::End(TagEnd::Table) => Some(Token::TableEnd),
            Event::Start(Tag::TableHead) => Some(Token::TableHeadStart),
            Event::End(TagEnd::TableHead) => Some(Token::TableHeadEnd),
            Event::Start(Tag::TableRow) => Some(Token::TableRowStart),
            Event::End(TagEnd::TableRow) => Some(Token::TableRowEnd),
            Event::Start(Tag::TableCell) => Some(Token::TableCellStart),
            Event::End(TagEnd::TableCell) => Some(Token::TableCellEnd),
            Event::SoftBreak => Some(Token::SoftBreak),
            _ => None,
        };
        if let Some(token) = token {
            push_token(&mut tokens, token);
        }
    }
    tokens
}

fn assert_differential(text: &str) -> GrammarOutput {
    let output = parse_with_fuel(text, 3);
    assert_eq!(
        own_tokens(text, &output),
        pulldown_tokens(text),
        "supported-subset differential failed for {text:?}"
    );
    output
}

#[test]
fn representative_paragraph_code_emphasis_and_strong_match_pulldown_0134() {
    let cases = [
        "plain text",
        "before *em* and **strong** after",
        "_left_ and __right__",
        "a_b_c keeps intraword underscores",
        "before ` code ` after",
        "`a  b` and ``x ` y``",
        r"\*literal\* and *live*",
        "*outer **inner** tail*",
        "foo*bar*baz",
        "(_bar_)",
        "**outer *inner* tail**",
        "`*literal*` and *`code`*",
        "foo__bar__baz",
    ];
    for case in cases {
        let output = assert_differential(case);
        assert_eq!(output.classification, GrammarClassification::Paragraph);
        assert_eq!(
            output.records.records().count(),
            output.records.record_count()
        );
    }
}

#[test]
fn empty_document_matches_pulldown_without_inventing_a_paragraph() {
    let output = assert_differential("");
    assert_eq!(output.classification, GrammarClassification::Empty);
    assert_eq!(output.records.record_count(), 0);
}

#[test]
fn real_gfm_table_path_matches_pulldown_and_uses_official_pipe_escape_rule() {
    // GFM requires a pipe inside code to be source-escaped. The shared lexer
    // suppresses that escaped pipe as a table delimiter, and this grammar path
    // consumes the same event root for both table and inline decisions.
    let text = "| left *em* | `c\\|d` |\n| :--- | ---: |\n| body | **strong** |";
    let output = assert_differential(text);
    assert_eq!(
        output.classification,
        GrammarClassification::Table {
            columns: 2,
            body_rows: 1
        }
    );
    let starts = output
        .records
        .records()
        .filter_map(|record| match record {
            GrammarRecord::TableCellStart {
                alignment,
                header,
                column,
                ..
            } if header => Some((column, alignment)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(starts, vec![(0, Alignment::Left), (1, Alignment::Right)]);

    let pulldown_alignments = Parser::new_ext(text, Options::ENABLE_TABLES)
        .find_map(|event| match event {
            Event::Start(Tag::Table(alignments)) => Some(alignments),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        pulldown_alignments,
        vec![PulldownAlignment::Left, PulldownAlignment::Right]
    );
}

#[test]
fn table_variants_cover_no_outer_pipes_header_only_crlf_and_literal_pipe() {
    for text in [
        "a | b\n:- | -:\nc | d",
        "| a | b |\n| - | - |",
        "| f\\|oo | b |\n| - | - |\n| c | d |",
        "| a | b |\r\n| :- | -: |\r\n| c | d |\r\n",
        "| a | `b|c` |\n| - | - |",
    ] {
        assert_differential(text);
    }
}

#[test]
fn incomplete_typing_states_are_stable_literal_paragraphs() {
    for text in [
        "typing *",
        "typing **",
        "typing `code",
        "typing \\",
        "a | b\n-- | x--",
    ] {
        let output = assert_differential(text);
        assert_eq!(output.classification, GrammarClassification::Paragraph);
    }
}

#[test]
fn every_poll_is_resumable_at_one_transition_and_work_is_fully_charged() {
    let text = "left *em* and `code` right";
    let output = parse_with_fuel(text, 1);
    let receipt = output.receipt;
    assert_eq!(receipt.logical_bytes_inspected, text.len() * 2);
    assert_eq!(receipt.source_bytes_inspected, text.len() * 2);
    assert_eq!(receipt.virtual_bytes_inspected, 0);
    assert_eq!(receipt.excluded_source_bytes_inspected, 0);
    assert!(receipt.cursor_transitions >= text.len() * 2 + 2);
    assert!(receipt.lexical_events_examined >= 6);
    assert!(receipt.parser_transitions >= receipt.cursor_transitions);
    assert!(receipt.index_units > 0);
    assert_eq!(receipt.unmetered_upstream_allocation_sites, 4);
    assert!(receipt.grammar_allocation_units >= 5);
    assert_eq!(receipt.hash_units, receipt.output_payload_bytes);
    let page_index_copy = output.records.page_count() * std::mem::size_of::<Arc<()>>();
    assert_eq!(
        receipt.copy_units,
        receipt.output_payload_bytes * 2 + page_index_copy
    );
    assert_eq!(receipt.encode_units, receipt.output_payload_bytes);
    assert_eq!(receipt.output_records, output.records.record_count());
    assert!(receipt.max_atomic_index_units <= MAX_ATOMIC_REGION_INDEX_UNITS);
}

#[test]
fn segmented_container_leaf_stays_logical_and_charges_excluded_source_walks() {
    let text = "> first\n> *second*";
    let source = Arc::new(PersistentSource::from_text(text));
    let second = text.find("*second*").unwrap();
    let mut builder = SegmentedLeafBuilder::new(source.clone());
    builder.push_source(2..7).unwrap();
    builder.push_virtual_newline(7).unwrap();
    builder.push_source(second..text.len()).unwrap();
    let leaf = builder.finish();

    let output = parse_leaf_with_fuel(&leaf, 1);
    assert_eq!(output.classification, GrammarClassification::Paragraph);
    assert_eq!(output.input.identity(), leaf.identity());
    let records = output.records.records().collect::<Vec<_>>();
    assert!(records
        .iter()
        .any(|record| matches!(record, GrammarRecord::EmphasisStart { .. })));
    let emphasis_start = records
        .iter()
        .find_map(|record| match record {
            GrammarRecord::EmphasisStart { start, .. } => Some(*start),
            _ => None,
        })
        .unwrap();
    let mut origin_cursor = output.input.cursor();
    let origin = loop {
        match origin_cursor.step() {
            flark_integrated_parser_slice::frontier::CursorStep::Byte(byte)
                if byte.logical_offset == emphasis_start =>
            {
                break byte.origin;
            }
            flark_integrated_parser_slice::frontier::CursorStep::Done => {
                panic!("emphasis offset must map through retained input")
            }
            _ => {}
        }
    };
    assert_eq!(
        origin,
        flark_integrated_parser_slice::frontier::LogicalOrigin::Source(
            source.anchor_at(second).unwrap()
        )
    );
    assert_eq!(output.receipt.logical_bytes_inspected, leaf.len() * 2);
    assert_eq!(output.receipt.virtual_bytes_inspected, 2);
    assert_eq!(output.receipt.excluded_source_bytes_inspected, 6);
    assert_eq!(
        output.receipt.source_bytes_inspected,
        (leaf.len() - 1) * 2 + 6
    );
}

#[test]
fn grammar_seeks_large_physical_gaps_and_charges_index_work_not_skipped_bytes() {
    const GAP: usize = 1024 * 1024;
    let text = format!("a{}b", "x".repeat(GAP));
    let source = Arc::new(PersistentSource::from_text(&text));
    let mut builder = SegmentedLeafBuilder::new(source);
    builder.push_source(0..1).unwrap();
    builder.push_virtual_newline(1).unwrap();
    builder.push_source(text.len() - 1..text.len()).unwrap();
    let leaf = builder.finish();

    let output = parse_leaf_with_fuel(&leaf, 1);
    assert_eq!(output.input.identity(), leaf.identity());
    assert_eq!(output.receipt.logical_bytes_inspected, 6);
    assert_eq!(output.receipt.virtual_bytes_inspected, 2);
    assert_eq!(output.receipt.excluded_source_bytes_inspected, 2);
    assert_eq!(output.receipt.source_bytes_skipped, 2 * (GAP - 1));
    assert!(output.receipt.source_index_nodes_examined > 0);
    assert!(output.receipt.cursor_transitions < 32);
}

#[test]
fn event_density_and_leaf_size_bounds_fall_back_without_unbounded_atomic_work() {
    let punctuation = "* ".repeat(MAX_INLINE_EVENTS_PER_REGION + 1);
    let dense = parse_with_fuel(&punctuation, 5);
    assert_eq!(dense.classification, GrammarClassification::Paragraph);
    assert!(dense.records.records().any(|record| matches!(
        record,
        GrammarRecord::LiteralFallback {
            reason: FallbackReason::TooManyInlineEvents,
            ..
        }
    )));
    assert_eq!(dense.receipt.max_atomic_index_units, 0);

    let huge = "x".repeat(MAX_GRAMMAR_LEAF_BYTES + 1);
    let oversized = parse_with_fuel(&huge, 1);
    assert_eq!(
        oversized.classification,
        GrammarClassification::LiteralFallback(FallbackReason::LeafTooLarge)
    );
    assert_eq!(oversized.receipt.logical_bytes_inspected, 0);
    assert_eq!(oversized.receipt.cursor_transitions, 0);
    assert_eq!(oversized.receipt.unmetered_upstream_allocation_sites, 0);
    assert_eq!(
        own_tokens(&huge, &oversized),
        vec![
            Token::ParagraphStart,
            Token::Text(huge),
            Token::ParagraphEnd,
        ]
    );
}

#[test]
fn unsupported_triple_emphasis_is_explicit_not_misparsed_as_supported() {
    let output = parse_with_fuel("***both***", 2);
    assert!(output.records.records().any(|record| matches!(
        record,
        GrammarRecord::LiteralFallback {
            reason: FallbackReason::UnsupportedEmphasisRun,
            ..
        }
    )));
    assert_eq!(
        own_tokens("***both***", &output),
        vec![
            Token::ParagraphStart,
            Token::Text("***both***".to_owned()),
            Token::ParagraphEnd,
        ]
    );
}

#[test]
fn partial_mixed_length_emphasis_is_explicit_fallback_not_silent_divergence() {
    for text in ["**foo*", "*foo**", "*foo **bar* baz**"] {
        let output = parse_with_fuel(text, 2);
        assert!(output.records.records().any(|record| matches!(
            record,
            GrammarRecord::LiteralFallback {
                reason: FallbackReason::UnsupportedEmphasisInteraction,
                ..
            }
        )));
        assert_eq!(
            own_tokens(text, &output),
            vec![
                Token::ParagraphStart,
                Token::Text(text.to_owned()),
                Token::ParagraphEnd,
            ]
        );
    }
}
