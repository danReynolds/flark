use crate::input::LogicalLeaf;
use crate::model::{CancellationToken, Fact, FactKind, MemoryReceipt, ParsePoll, ReferenceTable};
use flark_reference_label_service::ReferenceLabelAccumulator;
use std::collections::HashMap;
use std::mem::size_of;
use std::ops::Range;
use std::sync::Arc;
use unicode_categories::UnicodeCategories;

const NONE_INDEX: u32 = u32::MAX;
const MAX_LINK_NESTING: u8 = 32;
const MAX_PACKED_LEAF_BYTES: usize = (u32::MAX >> Token::KIND_BITS) as usize;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
enum TokenKind {
    Star,
    Underscore,
    Backtick,
    OpenBracket,
    CloseBracket,
}

#[derive(Clone, Copy, Debug)]
struct Token {
    start: u32,
    len_and_kind: u32,
    next_same: u32,
}

impl Token {
    const KIND_BITS: u32 = 3;
    const KIND_MASK: u32 = (1 << Self::KIND_BITS) - 1;

    fn new(start: usize, len: usize, kind: TokenKind) -> Self {
        debug_assert!(len > 0);
        debug_assert!(len <= (u32::MAX >> Self::KIND_BITS) as usize);
        Self {
            start: u32::try_from(start).expect("leaf size is checked at engine construction"),
            len_and_kind: (u32::try_from(len).expect("run is no longer than its leaf")
                << Self::KIND_BITS)
                | kind as u32,
            next_same: NONE_INDEX,
        }
    }

    fn kind(self) -> TokenKind {
        match self.len_and_kind & Self::KIND_MASK {
            0 => TokenKind::Star,
            1 => TokenKind::Underscore,
            2 => TokenKind::Backtick,
            3 => TokenKind::OpenBracket,
            4 => TokenKind::CloseBracket,
            _ => unreachable!("packed token kind is validated by construction"),
        }
    }

    fn end(self) -> usize {
        self.start as usize + self.len()
    }

    fn set_len(&mut self, len: usize) {
        let kind = self.len_and_kind & Self::KIND_MASK;
        self.len_and_kind =
            (u32::try_from(len).expect("run is no longer than its leaf") << Self::KIND_BITS) | kind;
    }

    fn range(self) -> Range<usize> {
        self.start as usize..self.end()
    }

    fn len(self) -> usize {
        (self.len_and_kind >> Self::KIND_BITS) as usize
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Phase {
    Lex,
    CodeIndex,
    Code,
    Links,
    Emphasis,
    Ready,
    Cancelled,
}

#[derive(Clone, Debug)]
struct CodeInspect {
    opener: usize,
    closer: usize,
    pos: usize,
    any_non_space: bool,
}

#[derive(Clone, Debug)]
struct CodeState {
    cursor: usize,
    inspect: Option<CodeInspect>,
}

#[derive(Clone, Copy, Debug)]
struct Bracket {
    token: usize,
    disabled: bool,
}

#[derive(Debug)]
struct LinksState {
    cursor: usize,
    brackets: Vec<Bracket>,
    code_range: usize,
    skip_until: usize,
    candidate: Option<LinkCandidate>,
    disable_cursor: Option<usize>,
}

#[derive(Debug)]
struct LinkCandidate {
    opener: Token,
    closer: Token,
    mode: LinkCandidateMode,
}

#[derive(Debug)]
enum LinkCandidateMode {
    Start,
    Inline(InlineCandidate),
    VisibleLabel(LabelNormalizer),
    ExplicitLabel {
        visible: String,
        visible_valid: bool,
        scanner: ExplicitLabelScanner,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum InlineStage {
    DestinationLead,
    AngleDestination,
    BareDestination,
    AfterDestination,
    Title,
    AfterTitle,
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Debug)]
struct InlineCandidate {
    pos: usize,
    stage: InlineStage,
    destination_start: usize,
    destination_end: usize,
    title: Option<Range<usize>>,
    title_delimiter: u8,
    nesting: u8,
    escaped: bool,
    separator_seen: bool,
    separator_newline_seen: bool,
    title_newline_seen: bool,
}

#[derive(Debug)]
struct LabelNormalizer {
    pos: usize,
    end: usize,
    accumulator: ReferenceLabelAccumulator,
    valid: bool,
}

#[derive(Debug)]
struct ExplicitLabelScanner {
    start: usize,
    pos: usize,
    accumulator: ReferenceLabelAccumulator,
    escaped: bool,
}

#[derive(Clone, Copy, Debug)]
struct Delimiter {
    token: Token,
    remaining: usize,
    run_length: usize,
    both: bool,
}

#[derive(Clone, Debug)]
struct PendingDelimiter {
    token: Token,
    remaining: usize,
    used_close: usize,
    run_length: usize,
    can_open: bool,
    can_close: bool,
    search_index: Option<usize>,
    lower_bound: usize,
}

#[derive(Clone, Debug)]
struct EmitMatch {
    opener: Delimiter,
    close_token: Token,
    match_count: usize,
    remaining_to_emit: usize,
    opener_consumed: usize,
    closer_consumed: usize,
}

#[derive(Clone, Debug)]
struct EmphasisState {
    cursor: usize,
    stack: Vec<Delimiter>,
    lower_bounds: [usize; 9],
    code_range: usize,
    link_range: usize,
    pending: Option<PendingDelimiter>,
    emit: Option<EmitMatch>,
}

/// A bounded, cancellable inline parser experiment.
///
/// It intentionally has no general tree or parent/sibling mutation API. The
/// only retained representations are a compact lexical tape, value stacks,
/// exclusion intervals, and direct facts.
#[derive(Debug)]
pub struct InlineEngine {
    leaf: LogicalLeaf,
    references: Arc<ReferenceTable>,
    phase: Phase,
    tokens: Vec<Token>,
    facts: Vec<Fact>,
    code_ranges: Vec<Range<usize>>,
    link_exclusions: Vec<Range<usize>>,
    lex_pos: usize,
    escape_next: bool,
    in_plain_run: bool,
    plain_runs: usize,
    code_index_cursor: usize,
    last_backtick: HashMap<u32, u32>,
    code: CodeState,
    links: LinksState,
    emphasis: EmphasisState,
    polls: usize,
    max_poll_work: usize,
    peak_stack_capacity_bytes: usize,
    peak_string_bytes: usize,
}

impl InlineEngine {
    /// Creates an engine for a logical leaf.
    ///
    /// # Panics
    ///
    /// Panics when the leaf exceeds the spike's packed offset limit (roughly
    /// 512 MiB). Production chunks would use leaf-local offsets below this.
    #[must_use]
    pub fn new(leaf: LogicalLeaf, references: Arc<ReferenceTable>) -> Self {
        assert!(
            leaf.len() <= MAX_PACKED_LEAF_BYTES,
            "the spike uses packed 32-bit leaf-relative offsets"
        );
        Self {
            leaf,
            references,
            phase: Phase::Lex,
            tokens: Vec::new(),
            facts: Vec::new(),
            code_ranges: Vec::new(),
            link_exclusions: Vec::new(),
            lex_pos: 0,
            escape_next: false,
            in_plain_run: false,
            plain_runs: 0,
            code_index_cursor: 0,
            last_backtick: HashMap::new(),
            code: CodeState {
                cursor: 0,
                inspect: None,
            },
            links: LinksState {
                cursor: 0,
                brackets: Vec::new(),
                code_range: 0,
                skip_until: 0,
                candidate: None,
                disable_cursor: None,
            },
            emphasis: EmphasisState {
                cursor: 0,
                stack: Vec::new(),
                lower_bounds: [0; 9],
                code_range: 0,
                link_range: 0,
                pending: None,
                emit: None,
            },
            polls: 0,
            max_poll_work: 0,
            peak_stack_capacity_bytes: 0,
            peak_string_bytes: 0,
        }
    }

    /// Advances by at most `fuel` source-byte or value-state transitions.
    /// A UTF-8 scalar lookup may inspect at most four bytes as one transition.
    pub fn resume(&mut self, fuel: usize, cancellation: &CancellationToken) -> ParsePoll {
        if self.phase == Phase::Ready {
            return ParsePoll::Ready { work: 0 };
        }
        if self.phase == Phase::Cancelled {
            return ParsePoll::Cancelled { work: 0 };
        }
        self.polls += 1;
        let mut work = 0usize;
        let mut free_transitions = 0u8;
        while work < fuel {
            if cancellation.is_cancelled() {
                self.phase = Phase::Cancelled;
                self.observe_peaks();
                self.max_poll_work = self.max_poll_work.max(work);
                return ParsePoll::Cancelled { work };
            }
            let consumed = match self.phase {
                Phase::Lex => self.step_lex(),
                Phase::CodeIndex => self.step_code_index(),
                Phase::Code => self.step_code(),
                Phase::Links => self.step_links(),
                Phase::Emphasis => self.step_emphasis(),
                Phase::Ready => {
                    self.observe_peaks();
                    self.max_poll_work = self.max_poll_work.max(work);
                    return ParsePoll::Ready { work };
                }
                Phase::Cancelled => {
                    self.observe_peaks();
                    self.max_poll_work = self.max_poll_work.max(work);
                    return ParsePoll::Cancelled { work };
                }
            };
            if consumed {
                work += 1;
                free_transitions = 0;
            } else {
                free_transitions += 1;
                debug_assert!(
                    free_transitions <= 8,
                    "phase transition failed to make progress"
                );
            }
        }
        self.observe_peaks();
        self.max_poll_work = self.max_poll_work.max(work);
        if self.phase == Phase::Ready {
            ParsePoll::Ready { work }
        } else {
            ParsePoll::Pending { work }
        }
    }

    #[must_use]
    pub fn is_ready(&self) -> bool {
        self.phase == Phase::Ready
    }

    #[must_use]
    pub fn facts(&self) -> &[Fact] {
        &self.facts
    }

    /// Returns a deterministic source order for differential tests and delta
    /// assembly. It is intentionally separate from `resume`; production would
    /// store each fact class in an ordered persistent chunk rather than sort.
    #[must_use]
    pub fn canonical_facts(&self) -> Vec<Fact> {
        let mut facts = self.facts.clone();
        facts.sort_by_key(Fact::sort_key);
        facts
    }

    #[must_use]
    pub fn leaf(&self) -> &LogicalLeaf {
        &self.leaf
    }

    #[must_use]
    pub fn plain_run_count(&self) -> usize {
        self.plain_runs
    }

    #[must_use]
    pub fn memory_receipt(&self) -> MemoryReceipt {
        let token_capacity_bytes = self.tokens.capacity() * size_of::<Token>();
        let fact_capacity_bytes = self.facts.capacity() * size_of::<Fact>();
        let interval_bytes = (self.code_ranges.capacity() + self.link_exclusions.capacity())
            * size_of::<Range<usize>>();
        let map_bytes = self.last_backtick.capacity() * (size_of::<u32>() * 2 + size_of::<usize>());
        let total = token_capacity_bytes
            + fact_capacity_bytes
            + interval_bytes
            + map_bytes
            + self.peak_stack_capacity_bytes
            + self.peak_string_bytes;
        MemoryReceipt {
            source_bytes_excluded: self.leaf.source_len(),
            token_count: self.tokens.len(),
            token_capacity_bytes,
            fact_count: self.facts.len(),
            fact_capacity_bytes,
            retained_stack_capacity_bytes: self.peak_stack_capacity_bytes,
            retained_string_bytes: self.peak_string_bytes,
            total_retained_auxiliary_bytes: total,
            polls: self.polls,
            max_poll_work: self.max_poll_work,
        }
    }

    fn observe_peaks(&mut self) {
        let stack_bytes = self.links.brackets.capacity() * size_of::<Bracket>()
            + self.emphasis.stack.capacity() * size_of::<Delimiter>();
        self.peak_stack_capacity_bytes = self.peak_stack_capacity_bytes.max(stack_bytes);
        let candidate_bytes = self
            .links
            .candidate
            .as_ref()
            .map_or(0, LinkCandidate::string_capacity);
        self.peak_string_bytes = self.peak_string_bytes.max(candidate_bytes);
    }

    fn step_lex(&mut self) -> bool {
        let Some(byte) = self.leaf.byte_at(self.lex_pos) else {
            self.phase = Phase::CodeIndex;
            return false;
        };
        let pos = self.lex_pos;
        self.lex_pos += 1;
        let escaped = self.escape_next;
        self.escape_next = false;

        let kind = match byte {
            b'*' => Some(TokenKind::Star),
            b'_' => Some(TokenKind::Underscore),
            b'`' => Some(TokenKind::Backtick),
            b'[' => Some(TokenKind::OpenBracket),
            b']' => Some(TokenKind::CloseBracket),
            _ => None,
        };
        if byte == b'\\' && !escaped {
            self.escape_next = true;
        }
        if escaped || kind.is_none() {
            if !self.in_plain_run {
                self.in_plain_run = true;
                self.plain_runs += 1;
            }
            return true;
        }
        self.in_plain_run = false;
        let kind = kind.expect("checked above");
        if matches!(
            kind,
            TokenKind::Star | TokenKind::Underscore | TokenKind::Backtick
        ) {
            if let Some(last) = self.tokens.last_mut() {
                if last.kind() == kind && last.end() == pos {
                    last.set_len(last.len() + 1);
                    return true;
                }
            }
        }
        self.tokens.push(Token::new(pos, 1, kind));
        true
    }

    fn step_code_index(&mut self) -> bool {
        let Some(token) = self.tokens.get(self.code_index_cursor).copied() else {
            self.phase = Phase::Code;
            return false;
        };
        let current = u32::try_from(self.code_index_cursor)
            .expect("token count cannot exceed the checked leaf length");
        self.code_index_cursor += 1;
        if token.kind() == TokenKind::Backtick {
            let count = u32::try_from(token.len()).expect("run is no longer than its leaf");
            if let Some(previous) = self.last_backtick.insert(count, current) {
                self.tokens[previous as usize].next_same = current;
            }
        }
        true
    }

    fn step_code(&mut self) -> bool {
        if let Some(inspect) = self.code.inspect.as_mut() {
            let closer = self.tokens[inspect.closer];
            if inspect.pos < closer.start as usize {
                let byte = self.leaf.byte_at(inspect.pos).unwrap_or_default();
                if !matches!(byte, b' ' | b'\n' | b'\r') {
                    inspect.any_non_space = true;
                }
                inspect.pos += 1;
                return true;
            }
            let opener = self.tokens[inspect.opener];
            let content = opener.end()..closer.start as usize;
            let leading_space = self.leaf.byte_at(content.start) == Some(b' ');
            let trailing_space = content
                .end
                .checked_sub(1)
                .and_then(|offset| self.leaf.byte_at(offset))
                == Some(b' ');
            self.facts.push(Fact {
                range: opener.start as usize..closer.end(),
                kind: FactKind::CodeSpan {
                    opener: opener.range(),
                    content: content.clone(),
                    closer: closer.range(),
                    trim_one_space: leading_space && trailing_space && inspect.any_non_space,
                },
            });
            self.code_ranges.push(opener.start as usize..closer.end());
            self.code.cursor = inspect.closer + 1;
            self.code.inspect = None;
            return false;
        }

        let Some(token) = self.tokens.get(self.code.cursor).copied() else {
            self.phase = Phase::Links;
            return false;
        };
        let token_index = self.code.cursor;
        self.code.cursor += 1;
        if token.kind() == TokenKind::Backtick && token.next_same != NONE_INDEX {
            let closer = token.next_same as usize;
            self.code.inspect = Some(CodeInspect {
                opener: token_index,
                closer,
                pos: token.end(),
                any_non_space: false,
            });
        }
        true
    }

    fn step_links(&mut self) -> bool {
        if let Some(cursor) = self.links.disable_cursor {
            if let Some(bracket) = self.links.brackets.get_mut(cursor) {
                bracket.disabled = true;
                self.links.disable_cursor = Some(cursor + 1);
                return true;
            }
            self.links.disable_cursor = None;
        }
        if let Some(mut candidate) = self.links.candidate.take() {
            let outcome = candidate.step(&self.leaf, &self.references);
            self.peak_string_bytes = self.peak_string_bytes.max(candidate.string_capacity());
            match outcome {
                CandidateOutcome::Pending => {
                    self.links.candidate = Some(candidate);
                }
                CandidateOutcome::Inline { fact, suffix } => {
                    self.links.skip_until = self.links.skip_until.max(suffix.end);
                    self.link_exclusions.push(suffix);
                    self.facts.push(fact);
                    self.links.disable_cursor = Some(0);
                }
                CandidateOutcome::Reference {
                    fact,
                    suffix,
                    resolved,
                } => {
                    if resolved {
                        self.links.skip_until = self.links.skip_until.max(suffix.end);
                        if !suffix.is_empty() {
                            self.link_exclusions.push(suffix);
                        }
                        self.links.disable_cursor = Some(0);
                    }
                    self.facts.push(fact);
                }
            }
            return true;
        }

        let Some(token) = self.tokens.get(self.links.cursor).copied() else {
            self.phase = Phase::Emphasis;
            return false;
        };
        let token_index = self.links.cursor;
        self.links.cursor += 1;

        while self
            .code_ranges
            .get(self.links.code_range)
            .is_some_and(|range| range.end <= token.start as usize)
        {
            self.links.code_range += 1;
        }
        if (token.start as usize) < self.links.skip_until
            || self
                .code_ranges
                .get(self.links.code_range)
                .is_some_and(|range| range.contains(&(token.start as usize)))
        {
            return true;
        }

        match token.kind() {
            TokenKind::OpenBracket => {
                self.links.brackets.push(Bracket {
                    token: token_index,
                    disabled: false,
                });
            }
            TokenKind::CloseBracket => {
                if let Some(bracket) = self.links.brackets.pop() {
                    if !bracket.disabled {
                        self.links.candidate = Some(LinkCandidate {
                            opener: self.tokens[bracket.token],
                            closer: token,
                            mode: LinkCandidateMode::Start,
                        });
                    }
                }
            }
            _ => {}
        }
        true
    }

    #[allow(clippy::too_many_lines)]
    fn step_emphasis(&mut self) -> bool {
        if let Some(mut emit) = self.emphasis.emit.take() {
            let increment = if emit.remaining_to_emit > 1 { 2 } else { 1 };
            let opener_end = emit.opener.token.end() - emit.opener_consumed;
            let opener = opener_end - increment..opener_end;
            let closer_start = emit.close_token.start as usize + emit.closer_consumed;
            let closer = closer_start..closer_start + increment;
            let kind = if increment == 2 {
                FactKind::Strong {
                    opener: opener.clone(),
                    closer: closer.clone(),
                }
            } else {
                FactKind::Emphasis {
                    opener: opener.clone(),
                    closer: closer.clone(),
                }
            };
            self.facts.push(Fact {
                range: opener.start..closer.end,
                kind,
            });
            emit.opener_consumed += increment;
            emit.closer_consumed += increment;
            emit.remaining_to_emit -= increment;
            if emit.remaining_to_emit == 0 {
                let leftover = emit.opener.remaining - emit.match_count;
                if leftover > 0 {
                    let mut token = emit.opener.token;
                    token.set_len(leftover);
                    self.emphasis.stack.push(Delimiter {
                        token,
                        remaining: leftover,
                        ..emit.opener
                    });
                }
                if let Some(pending) = self.emphasis.pending.as_mut() {
                    pending.remaining -= emit.match_count;
                    pending.used_close += emit.match_count;
                    pending.search_index = None;
                }
            } else {
                self.emphasis.emit = Some(emit);
            }
            return true;
        }

        if let Some(mut pending) = self.emphasis.pending.take() {
            if pending.can_close && pending.remaining > 0 {
                if pending.search_index.is_none() {
                    pending.lower_bound = get_lower_bound(
                        &self.emphasis.lower_bounds,
                        pending.token.kind(),
                        pending.run_length,
                        pending.can_open && pending.can_close,
                    )
                    .min(self.emphasis.stack.len());
                    pending.search_index = Some(self.emphasis.stack.len());
                }
                if let Some(search_next) = pending.search_index {
                    if search_next > pending.lower_bound {
                        let search_index = search_next - 1;
                        let opener = self.emphasis.stack[search_index];
                        if delimiter_matches(&pending, opener) {
                            self.emphasis.stack.truncate(search_index);
                            clamp_lower_bounds(
                                &mut self.emphasis.lower_bounds,
                                self.emphasis.stack.len(),
                            );
                            let match_count = pending.remaining.min(opener.remaining);
                            self.emphasis.emit = Some(EmitMatch {
                                opener,
                                close_token: pending.token,
                                match_count,
                                remaining_to_emit: match_count,
                                opener_consumed: 0,
                                closer_consumed: pending.used_close,
                            });
                            self.emphasis.pending = Some(pending);
                            return true;
                        }
                        pending.search_index = Some(search_index);
                        self.emphasis.pending = Some(pending);
                        return true;
                    }
                }
                set_lower_bound(
                    &mut self.emphasis.lower_bounds,
                    pending.token.kind(),
                    pending.run_length,
                    pending.can_open && pending.can_close,
                    self.emphasis.stack.len(),
                );
            }
            if pending.can_open && pending.remaining > 0 {
                self.emphasis.stack.push(Delimiter {
                    token: pending.token,
                    remaining: pending.remaining,
                    run_length: pending.run_length,
                    both: pending.can_open && pending.can_close,
                });
            }
            return true;
        }

        let Some(token) = self.tokens.get(self.emphasis.cursor).copied() else {
            self.phase = Phase::Ready;
            return false;
        };
        self.emphasis.cursor += 1;

        while self
            .code_ranges
            .get(self.emphasis.code_range)
            .is_some_and(|range| range.end <= token.start as usize)
        {
            self.emphasis.code_range += 1;
        }
        while self
            .link_exclusions
            .get(self.emphasis.link_range)
            .is_some_and(|range| range.end <= token.start as usize)
        {
            self.emphasis.link_range += 1;
        }
        let excluded = self
            .code_ranges
            .get(self.emphasis.code_range)
            .is_some_and(|range| range.contains(&(token.start as usize)))
            || self
                .link_exclusions
                .get(self.emphasis.link_range)
                .is_some_and(|range| range.contains(&(token.start as usize)));
        if excluded || !matches!(token.kind(), TokenKind::Star | TokenKind::Underscore) {
            return true;
        }
        let (can_open, can_close) = delimiter_flanking(&self.leaf, token);
        self.emphasis.pending = Some(PendingDelimiter {
            token,
            remaining: token.len(),
            used_close: 0,
            run_length: token.len(),
            can_open,
            can_close,
            search_index: None,
            lower_bound: 0,
        });
        true
    }
}

#[derive(Clone, Debug)]
enum CandidateOutcome {
    Pending,
    Inline {
        fact: Fact,
        suffix: Range<usize>,
    },
    Reference {
        fact: Fact,
        suffix: Range<usize>,
        resolved: bool,
    },
}

impl LinkCandidate {
    #[allow(clippy::too_many_lines)]
    fn step(&mut self, leaf: &LogicalLeaf, references: &ReferenceTable) -> CandidateOutcome {
        match &mut self.mode {
            LinkCandidateMode::Start => {
                let next = self.closer.end();
                if leaf.byte_at(next) == Some(b'(') {
                    self.mode = LinkCandidateMode::Inline(InlineCandidate::new(next + 1));
                } else {
                    self.mode = LinkCandidateMode::VisibleLabel(LabelNormalizer::new(
                        self.opener.end(),
                        self.closer.start as usize,
                    ));
                }
                CandidateOutcome::Pending
            }
            LinkCandidateMode::Inline(scanner) => match scanner.step(leaf) {
                InlineStep::Pending => CandidateOutcome::Pending,
                InlineStep::Failed => {
                    self.mode = LinkCandidateMode::VisibleLabel(LabelNormalizer::new(
                        self.opener.end(),
                        self.closer.start as usize,
                    ));
                    CandidateOutcome::Pending
                }
                InlineStep::Complete { end } => {
                    let fact = Fact {
                        range: self.opener.start as usize..end,
                        kind: FactKind::InlineLink {
                            label: self.opener.end()..self.closer.start as usize,
                            destination: scanner.destination_start..scanner.destination_end,
                            title: scanner.title.clone(),
                        },
                    };
                    CandidateOutcome::Inline {
                        fact,
                        suffix: self.closer.end()..end,
                    }
                }
            },
            LinkCandidateMode::VisibleLabel(scanner) => {
                if !scanner.step(leaf) {
                    return CandidateOutcome::Pending;
                }
                let visible = scanner.accumulator.as_str().to_owned();
                let after = self.closer.end();
                if leaf.byte_at(after) == Some(b'[') {
                    self.mode = LinkCandidateMode::ExplicitLabel {
                        visible,
                        visible_valid: scanner.valid,
                        scanner: ExplicitLabelScanner::new(after + 1),
                    };
                    CandidateOutcome::Pending
                } else {
                    reference_outcome(
                        self.opener,
                        self.closer,
                        references,
                        visible,
                        self.opener.end()..self.closer.start as usize,
                        self.opener.start as usize..self.closer.end(),
                        scanner.valid,
                    )
                }
            }
            LinkCandidateMode::ExplicitLabel {
                visible,
                visible_valid,
                scanner,
            } => match scanner.step(leaf) {
                ExplicitStep::Pending => CandidateOutcome::Pending,
                ExplicitStep::Complete { end } => {
                    let normalized = if scanner.accumulator.is_empty() {
                        visible.clone()
                    } else {
                        scanner.accumulator.as_str().to_owned()
                    };
                    let reference = if scanner.accumulator.is_empty() {
                        self.opener.end()..self.closer.start as usize
                    } else {
                        scanner.start..end - 1
                    };
                    let allow_resolution = !scanner.accumulator.is_empty() || *visible_valid;
                    reference_outcome(
                        self.opener,
                        self.closer,
                        references,
                        normalized,
                        reference,
                        self.opener.start as usize..end,
                        allow_resolution,
                    )
                }
                ExplicitStep::Failed => reference_outcome(
                    self.opener,
                    self.closer,
                    references,
                    visible.clone(),
                    self.opener.end()..self.closer.start as usize,
                    self.opener.start as usize..self.closer.end(),
                    *visible_valid,
                ),
            },
        }
    }

    fn string_capacity(&self) -> usize {
        match &self.mode {
            LinkCandidateMode::VisibleLabel(scanner) => scanner.accumulator.allocated_bytes(),
            LinkCandidateMode::ExplicitLabel {
                visible, scanner, ..
            } => visible.capacity() + scanner.accumulator.allocated_bytes(),
            LinkCandidateMode::Start | LinkCandidateMode::Inline(_) => 0,
        }
    }
}

fn reference_outcome(
    opener: Token,
    closer: Token,
    references: &ReferenceTable,
    normalized_label: String,
    reference: Range<usize>,
    attempted_range: Range<usize>,
    allow_resolution: bool,
) -> CandidateOutcome {
    let label = opener.end()..closer.start as usize;
    let dependency_id = if !allow_resolution || normalized_label.starts_with('^') {
        None
    } else {
        references.dependency_id(&normalized_label)
    };
    if let Some(dependency_id) = dependency_id {
        CandidateOutcome::Reference {
            fact: Fact {
                range: attempted_range.clone(),
                kind: FactKind::ReferenceLink {
                    label,
                    reference,
                    normalized_label,
                    dependency_id,
                },
            },
            suffix: closer.end()..attempted_range.end,
            resolved: true,
        }
    } else {
        CandidateOutcome::Reference {
            fact: Fact {
                range: attempted_range,
                kind: FactKind::UnresolvedReference {
                    label,
                    reference,
                    normalized_label,
                },
            },
            suffix: closer.end()..closer.end(),
            resolved: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum InlineStep {
    Pending,
    Complete { end: usize },
    Failed,
}

impl InlineCandidate {
    fn new(pos: usize) -> Self {
        Self {
            pos,
            stage: InlineStage::DestinationLead,
            destination_start: pos,
            destination_end: pos,
            title: None,
            title_delimiter: 0,
            nesting: 0,
            escaped: false,
            separator_seen: false,
            separator_newline_seen: false,
            title_newline_seen: false,
        }
    }

    #[allow(clippy::too_many_lines)]
    fn step(&mut self, leaf: &LogicalLeaf) -> InlineStep {
        let Some(byte) = leaf.byte_at(self.pos) else {
            return InlineStep::Failed;
        };
        match self.stage {
            InlineStage::DestinationLead => {
                if is_ascii_whitespace(byte) {
                    self.pos += 1;
                } else if byte == b')' {
                    self.destination_start = self.pos;
                    self.destination_end = self.pos;
                    self.pos += 1;
                    return InlineStep::Complete { end: self.pos };
                } else if byte == b'<' {
                    self.destination_start = self.pos + 1;
                    self.pos += 1;
                    self.stage = InlineStage::AngleDestination;
                } else {
                    self.destination_start = self.pos;
                    self.stage = InlineStage::BareDestination;
                }
            }
            InlineStage::AngleDestination => {
                if byte == b'>' {
                    self.destination_end = self.pos;
                    self.pos += 1;
                    self.stage = InlineStage::AfterDestination;
                } else if matches!(byte, b'<' | b'\n' | b'\r') {
                    return InlineStep::Failed;
                } else if byte == b'\\' {
                    self.pos += 1;
                    self.escaped = true;
                } else {
                    self.pos += 1;
                }
            }
            InlineStage::BareDestination => {
                if self.escaped {
                    self.escaped = false;
                    self.pos += 1;
                } else {
                    match byte {
                        b'\\' => {
                            self.escaped = true;
                            self.pos += 1;
                        }
                        b'(' => {
                            if self.nesting == MAX_LINK_NESTING {
                                return InlineStep::Failed;
                            }
                            self.nesting += 1;
                            self.pos += 1;
                        }
                        b')' if self.nesting > 0 => {
                            self.nesting -= 1;
                            self.pos += 1;
                        }
                        b')' => {
                            self.destination_end = self.pos;
                            self.pos += 1;
                            return InlineStep::Complete { end: self.pos };
                        }
                        b'<' | 0 => return InlineStep::Failed,
                        b if is_ascii_whitespace(b) => {
                            self.destination_end = self.pos;
                            self.separator_seen = true;
                            self.separator_newline_seen = matches!(b, b'\n' | b'\r');
                            self.pos += 1;
                            self.stage = InlineStage::AfterDestination;
                        }
                        _ => self.pos += 1,
                    }
                }
            }
            InlineStage::AfterDestination => {
                if is_ascii_whitespace(byte) {
                    let begins_linebreak = byte == b'\r'
                        || (byte == b'\n'
                            && self.pos.checked_sub(1).and_then(|pos| leaf.byte_at(pos))
                                != Some(b'\r'));
                    if begins_linebreak && self.separator_newline_seen {
                        return InlineStep::Failed;
                    }
                    self.separator_newline_seen |= begins_linebreak;
                    self.separator_seen = true;
                    self.pos += 1;
                } else if byte == b')' {
                    self.pos += 1;
                    return InlineStep::Complete { end: self.pos };
                } else if self.separator_seen && matches!(byte, b'\'' | b'"' | b'(') {
                    self.title_delimiter = if byte == b'(' { b')' } else { byte };
                    let start = self.pos + 1;
                    self.title = Some(start..start);
                    self.pos += 1;
                    self.stage = InlineStage::Title;
                } else {
                    return InlineStep::Failed;
                }
            }
            InlineStage::Title => {
                if self.escaped {
                    self.escaped = false;
                    self.pos += 1;
                } else if byte == b'\\' {
                    self.escaped = true;
                    self.pos += 1;
                } else if byte == self.title_delimiter {
                    if let Some(title) = self.title.as_mut() {
                        title.end = self.pos;
                    }
                    self.pos += 1;
                    self.stage = InlineStage::AfterTitle;
                } else if self.title_delimiter == b')' && byte == b'(' {
                    return InlineStep::Failed;
                } else if matches!(byte, b'\n' | b'\r') {
                    let begins_linebreak = byte == b'\r'
                        || (byte == b'\n'
                            && self.pos.checked_sub(1).and_then(|pos| leaf.byte_at(pos))
                                != Some(b'\r'));
                    if begins_linebreak && self.title_newline_seen {
                        return InlineStep::Failed;
                    }
                    self.title_newline_seen |= begins_linebreak;
                    self.pos += 1;
                } else {
                    if !matches!(byte, b' ' | b'\t') {
                        self.title_newline_seen = false;
                    }
                    self.pos += 1;
                }
            }
            InlineStage::AfterTitle => {
                if is_ascii_whitespace(byte) {
                    self.pos += 1;
                } else if byte == b')' {
                    self.pos += 1;
                    return InlineStep::Complete { end: self.pos };
                } else {
                    return InlineStep::Failed;
                }
            }
        }
        InlineStep::Pending
    }
}

impl LabelNormalizer {
    fn new(pos: usize, end: usize) -> Self {
        Self {
            pos,
            end,
            accumulator: ReferenceLabelAccumulator::with_source_byte_hint(end - pos),
            valid: true,
        }
    }

    /// Returns true after the complete range has been normalized.
    fn step(&mut self, leaf: &LogicalLeaf) -> bool {
        if self.pos >= self.end {
            return true;
        }
        let Some(ch) = leaf.char_at(self.pos) else {
            self.pos = self.end;
            return true;
        };
        self.pos = (self.pos + ch.len_utf8()).min(self.end);
        if self.valid {
            let contribution = leaf.raw_codepoint_contribution_at(self.pos - ch.len_utf8());
            self.valid = contribution
                .is_some_and(|contribution| self.accumulator.push(ch, contribution).is_ok());
        }
        self.pos >= self.end
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ExplicitStep {
    Pending,
    Complete { end: usize },
    Failed,
}

impl ExplicitLabelScanner {
    fn new(start: usize) -> Self {
        Self {
            start,
            pos: start,
            accumulator: ReferenceLabelAccumulator::new(),
            escaped: false,
        }
    }

    fn step(&mut self, leaf: &LogicalLeaf) -> ExplicitStep {
        let Some(byte) = leaf.byte_at(self.pos) else {
            return ExplicitStep::Failed;
        };
        if !self.escaped && byte == b']' {
            return ExplicitStep::Complete { end: self.pos + 1 };
        }
        if !self.escaped && byte == b'[' {
            return ExplicitStep::Failed;
        }
        let Some(ch) = leaf.char_at(self.pos) else {
            return ExplicitStep::Failed;
        };
        let width = ch.len_utf8();
        self.pos += width;
        if self.escaped {
            self.escaped = false;
        } else if ch == '\\' {
            self.escaped = true;
        }
        let Some(contribution) = leaf.raw_codepoint_contribution_at(self.pos - width) else {
            return ExplicitStep::Failed;
        };
        if self.accumulator.push(ch, contribution).is_err() {
            return ExplicitStep::Failed;
        }
        ExplicitStep::Pending
    }
}

const fn is_ascii_whitespace(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | b'\r')
}

fn delimiter_flanking(leaf: &LogicalLeaf, token: Token) -> (bool, bool) {
    let previous = leaf.char_before(token.start as usize);
    let next = leaf.char_at(token.end());
    let previous_whitespace = previous.is_none_or(char::is_whitespace);
    let next_whitespace = next.is_none_or(char::is_whitespace);
    let previous_punctuation = previous.is_some_and(UnicodeCategories::is_punctuation);
    let next_punctuation = next.is_some_and(UnicodeCategories::is_punctuation);

    let left_flanking =
        !next_whitespace && (!next_punctuation || previous_whitespace || previous_punctuation);
    let right_flanking =
        !previous_whitespace && (!previous_punctuation || next_whitespace || next_punctuation);
    if token.kind() == TokenKind::Underscore {
        (
            left_flanking && (!right_flanking || previous_punctuation),
            right_flanking && (!left_flanking || next_punctuation),
        )
    } else {
        (left_flanking, right_flanking)
    }
}

fn delimiter_matches(pending: &PendingDelimiter, opener: Delimiter) -> bool {
    if opener.token.kind() != pending.token.kind() {
        return false;
    }
    let both = pending.can_open && pending.can_close;
    (!both && !opener.both)
        || !(pending.run_length + opener.run_length).is_multiple_of(3)
        || pending.run_length.is_multiple_of(3)
}

fn get_lower_bound(lower_bounds: &[usize; 9], kind: TokenKind, count: usize, both: bool) -> usize {
    if kind == TokenKind::Underscore {
        let modulo = lower_bounds[6 + count % 3];
        if both {
            modulo
        } else {
            modulo.min(lower_bounds[0])
        }
    } else {
        let modulo = lower_bounds[2 + count % 3];
        if both {
            modulo
        } else {
            modulo.min(lower_bounds[1])
        }
    }
}

fn set_lower_bound(
    lower_bounds: &mut [usize; 9],
    kind: TokenKind,
    count: usize,
    both: bool,
    new_bound: usize,
) {
    if kind == TokenKind::Underscore {
        if both {
            lower_bounds[6 + count % 3] = new_bound;
        } else {
            lower_bounds[0] = new_bound;
        }
    } else {
        lower_bounds[2 + count % 3] = new_bound;
        if !both {
            lower_bounds[1] = new_bound;
        }
    }
}

fn clamp_lower_bounds(lower_bounds: &mut [usize; 9], len: usize) {
    for lower_bound in lower_bounds {
        *lower_bound = (*lower_bound).min(len);
    }
}
