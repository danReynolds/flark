use std::collections::HashMap;
use std::ops::Range;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Fact {
    pub range: Range<usize>,
    pub kind: FactKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FactKind {
    Emphasis {
        opener: Range<usize>,
        closer: Range<usize>,
    },
    Strong {
        opener: Range<usize>,
        closer: Range<usize>,
    },
    CodeSpan {
        opener: Range<usize>,
        content: Range<usize>,
        closer: Range<usize>,
        trim_one_space: bool,
    },
    InlineLink {
        label: Range<usize>,
        destination: Range<usize>,
        title: Option<Range<usize>>,
    },
    ReferenceLink {
        label: Range<usize>,
        reference: Range<usize>,
        normalized_label: String,
        dependency_id: u64,
    },
    UnresolvedReference {
        label: Range<usize>,
        reference: Range<usize>,
        normalized_label: String,
    },
}

impl Fact {
    #[must_use]
    pub fn sort_key(&self) -> (usize, usize, u8) {
        let rank = match self.kind {
            FactKind::CodeSpan { .. } => 0,
            FactKind::InlineLink { .. } => 1,
            FactKind::ReferenceLink { .. } => 2,
            FactKind::UnresolvedReference { .. } => 3,
            FactKind::Strong { .. } => 4,
            FactKind::Emphasis { .. } => 5,
        };
        (self.range.start, self.range.end, rank)
    }
}

#[derive(Clone, Debug, Default)]
pub struct ReferenceTable {
    definitions: HashMap<String, u64>,
}

impl ReferenceTable {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn define(&mut self, label: &str, dependency_id: u64) {
        self.definitions
            .entry(normalize_reference_label(label))
            .or_insert(dependency_id);
    }

    /// Install a block-parser-certified canonical label without normalizing or
    /// allocating a second key on the inline side.
    pub fn define_normalized(&mut self, normalized_label: String, dependency_id: u64) {
        self.definitions
            .entry(normalized_label)
            .or_insert(dependency_id);
    }

    #[must_use]
    pub fn dependency_id(&self, normalized_label: &str) -> Option<u64> {
        self.definitions.get(normalized_label).copied()
    }
}

#[must_use]
pub fn normalize_reference_label(label: &str) -> String {
    flark_reference_label_service::normalize_reference_label(label)
}

#[derive(Clone, Debug, Default)]
pub struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParsePoll {
    Pending { work: usize },
    Ready { work: usize },
    Cancelled { work: usize },
}

impl ParsePoll {
    #[must_use]
    pub const fn work(self) -> usize {
        match self {
            Self::Pending { work } | Self::Ready { work } | Self::Cancelled { work } => work,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MemoryReceipt {
    pub source_bytes_excluded: usize,
    pub token_count: usize,
    pub token_capacity_bytes: usize,
    pub fact_count: usize,
    pub fact_capacity_bytes: usize,
    pub retained_stack_capacity_bytes: usize,
    pub retained_string_bytes: usize,
    pub total_retained_auxiliary_bytes: usize,
    pub polls: usize,
    pub max_poll_work: usize,
}
