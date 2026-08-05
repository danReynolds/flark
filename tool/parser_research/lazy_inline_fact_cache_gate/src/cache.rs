use std::collections::VecDeque;
use std::fmt;
use std::mem::size_of;
use std::ops::Range;

use flark_comrak_inline_fragment_gate::{
    parse_inline_fragment, InlineFact, InlineFactKind, InlineFragment, InlineFragmentError,
    InlineFragmentRequest, InlineProfile,
};
use flark_relative_output_reuse_gate::PageId;

use crate::{
    DocumentError, IndexedDocument, LeafDescriptor, LeafVersion, ReferenceSnapshot,
    DEFAULT_LOGICAL_LEAF_BYTES,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum RetentionClass {
    #[default]
    Cold,
    Overscan,
    Visible,
    Active,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CacheStats {
    pub entries: usize,
    pub accounted_bytes: usize,
    pub facts: usize,
    pub projection_facts: usize,
    pub payload_bytes: usize,
    pub dependencies: usize,
    pub evictions: usize,
    pub dependency_invalidations: usize,
    pub generation_invalidations: usize,
    pub context_invalidations: usize,
    pub oversize_rejections: usize,
}

#[derive(Debug)]
struct CacheEntry {
    version: LeafVersion,
    class: RetentionClass,
    last_touch: u64,
    dynamic_bytes: usize,
    fragment: InlineFragment,
}

fn fragment_dynamic_bytes(fragment: &InlineFragment) -> usize {
    fragment.facts.capacity() * size_of::<InlineFact>()
        + fragment.projection_facts.capacity() * size_of::<InlineFact>()
        + fragment.payload.capacity()
        + fragment.reference_dependencies.capacity()
            * size_of::<flark_comrak_inline_fragment_gate::ReferenceDependency>()
        + fragment
            .reference_dependencies
            .iter()
            .map(|dependency| dependency.normalized_label.capacity())
            .sum::<usize>()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CacheInsert {
    Adopted { evicted: usize },
    TooLarge,
}

#[derive(Debug)]
pub struct FactCache {
    entries: Vec<CacheEntry>,
    maximum_bytes: usize,
    maximum_entries: usize,
    clock: u64,
    evictions: usize,
    dependency_invalidations: usize,
    generation_invalidations: usize,
    context_invalidations: usize,
    oversize_rejections: usize,
}

impl FactCache {
    #[must_use]
    pub fn new(maximum_bytes: usize, maximum_entries: usize) -> Self {
        let entries = Vec::with_capacity(maximum_entries);
        assert!(entries.capacity() * size_of::<CacheEntry>() <= maximum_bytes);
        Self {
            entries,
            maximum_bytes,
            maximum_entries,
            clock: 0,
            evictions: 0,
            dependency_invalidations: 0,
            generation_invalidations: 0,
            context_invalidations: 0,
            oversize_rejections: 0,
        }
    }

    fn fixed_bytes(&self) -> usize {
        self.entries.capacity() * size_of::<CacheEntry>()
    }

    fn dynamic_bytes(&self) -> usize {
        self.entries.iter().map(|entry| entry.dynamic_bytes).sum()
    }

    #[must_use]
    pub fn accounted_retained_bytes(&self) -> usize {
        self.fixed_bytes() + self.dynamic_bytes()
    }

    #[must_use]
    pub const fn maximum_bytes(&self) -> usize {
        self.maximum_bytes
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn set_classes(&mut self, desired: &[(PageId, RetentionClass)]) {
        for entry in &mut self.entries {
            entry.class = RetentionClass::Cold;
        }
        for (id, class) in desired {
            if let Some(entry) = self
                .entries
                .iter_mut()
                .find(|entry| entry.version.id == *id)
            {
                entry.class = entry.class.max(*class);
            }
        }
    }

    pub fn get(
        &mut self,
        version: LeafVersion,
        references: &ReferenceSnapshot,
    ) -> Option<&InlineFragment> {
        let index = self
            .entries
            .iter()
            .position(|entry| entry.version.id == version.id)?;
        if self.entries[index].version.content_generation != version.content_generation {
            self.entries.remove(index);
            self.generation_invalidations += 1;
            return None;
        }
        if self.entries[index].version.inline_context != version.inline_context {
            self.entries.remove(index);
            self.context_invalidations += 1;
            return None;
        }
        if !self.entries[index]
            .fragment
            .reference_dependencies
            .iter()
            .all(|dependency| references.dependency_is_current(dependency))
        {
            self.entries.remove(index);
            self.dependency_invalidations += 1;
            return None;
        }
        self.clock = self.clock.wrapping_add(1);
        self.entries[index].last_touch = self.clock;
        Some(&self.entries[index].fragment)
    }

    pub fn insert(
        &mut self,
        version: LeafVersion,
        class: RetentionClass,
        fragment: InlineFragment,
    ) -> CacheInsert {
        let dynamic_bytes = fragment_dynamic_bytes(&fragment);
        if self.fixed_bytes() + dynamic_bytes > self.maximum_bytes {
            self.oversize_rejections += 1;
            return CacheInsert::TooLarge;
        }
        if let Some(index) = self
            .entries
            .iter()
            .position(|entry| entry.version.id == version.id)
        {
            self.entries.remove(index);
        }
        let mut evicted = 0;
        while self.entries.len() == self.maximum_entries
            || self.accounted_retained_bytes() + dynamic_bytes > self.maximum_bytes
        {
            let Some(index) = self.eviction_candidate() else {
                self.oversize_rejections += 1;
                return CacheInsert::TooLarge;
            };
            self.entries.remove(index);
            self.evictions += 1;
            evicted += 1;
        }
        self.clock = self.clock.wrapping_add(1);
        self.entries.push(CacheEntry {
            version,
            class,
            last_touch: self.clock,
            dynamic_bytes,
            fragment,
        });
        debug_assert!(self.accounted_retained_bytes() <= self.maximum_bytes);
        CacheInsert::Adopted { evicted }
    }

    fn eviction_candidate(&self) -> Option<usize> {
        self.entries
            .iter()
            .enumerate()
            .min_by_key(|(_, entry)| (entry.class, entry.last_touch))
            .map(|(index, _)| index)
    }

    #[must_use]
    pub fn stats(&self) -> CacheStats {
        CacheStats {
            entries: self.entries.len(),
            accounted_bytes: self.accounted_retained_bytes(),
            facts: self
                .entries
                .iter()
                .map(|entry| entry.fragment.facts.len())
                .sum(),
            projection_facts: self
                .entries
                .iter()
                .map(|entry| entry.fragment.projection_facts.len())
                .sum(),
            payload_bytes: self
                .entries
                .iter()
                .map(|entry| entry.fragment.payload.len())
                .sum(),
            dependencies: self
                .entries
                .iter()
                .map(|entry| entry.fragment.reference_dependencies.len())
                .sum(),
            evictions: self.evictions,
            dependency_invalidations: self.dependency_invalidations,
            generation_invalidations: self.generation_invalidations,
            context_invalidations: self.context_invalidations,
            oversize_rejections: self.oversize_rejections,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DesiredLeaf {
    ordinal: usize,
    version: LeafVersion,
    class: RetentionClass,
}

#[derive(Clone, Debug)]
struct ParseRequest {
    ordinal: usize,
    version: LeafVersion,
    revision: u64,
    epoch: u64,
    class: RetentionClass,
}

#[derive(Clone, Debug)]
pub struct PreparedInlineJob {
    pub version: LeafVersion,
    pub revision: u64,
    pub epoch: u64,
    pub class: RetentionClass,
    pub logical: String,
}

impl PreparedInlineJob {
    pub fn run(
        self,
        references: &ReferenceSnapshot,
        expected_revision: u64,
    ) -> Result<InlineCompletion, InlineFragmentError> {
        let logical_bytes = self.logical.len();
        let fragment = parse_inline_fragment(InlineFragmentRequest {
            logical: &self.logical,
            leaf_id: self.version.id.0,
            kind: self.version.inline_context.parser_kind(),
            profile: InlineProfile::Gfm,
            reference_snapshot: references,
            revision: self.revision,
            expected_revision,
        })?;
        Ok(InlineCompletion {
            version: self.version,
            epoch: self.epoch,
            class: self.class,
            logical_bytes,
            fragment,
        })
    }
}

#[derive(Clone, Debug)]
pub struct InlineCompletion {
    pub version: LeafVersion,
    pub epoch: u64,
    pub class: RetentionClass,
    pub logical_bytes: usize,
    pub fragment: InlineFragment,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Adoption {
    Adopted { evicted: usize },
    StaleRevision,
    StaleLeaf,
    StaleWindow,
    StaleDependency,
    TooLarge,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ScheduleReceipt {
    pub epoch: u64,
    pub desired_leaves: usize,
    pub cache_hits: usize,
    pub queued: usize,
    pub source_visible: usize,
    pub prior_queue_collapsed: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BatchReceipt {
    pub parsed_leaves: usize,
    pub parsed_bytes: usize,
    pub facts: usize,
    pub projection_facts: usize,
    pub protocol_output_bytes: usize,
    pub adopted: usize,
    pub rejected: usize,
    pub evicted: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LeafPresentation {
    Exact {
        facts: usize,
        projection_facts: usize,
        task_list_markers: usize,
    },
    SourceVisible {
        pending: bool,
    },
}

#[derive(Debug)]
pub enum CacheGateError {
    Document(DocumentError),
    Inline(InlineFragmentError),
}

impl fmt::Display for CacheGateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Document(error) => error.fmt(formatter),
            Self::Inline(error) => write!(formatter, "inline parse failed: {error:?}"),
        }
    }
}

impl std::error::Error for CacheGateError {}

impl From<DocumentError> for CacheGateError {
    fn from(value: DocumentError) -> Self {
        Self::Document(value)
    }
}

impl From<InlineFragmentError> for CacheGateError {
    fn from(value: InlineFragmentError) -> Self {
        Self::Inline(value)
    }
}

#[derive(Debug)]
pub struct LazyInlineController {
    cache: FactCache,
    queue: VecDeque<ParseRequest>,
    desired: Vec<DesiredLeaf>,
    epoch: u64,
    stale_prepares: usize,
}

impl LazyInlineController {
    #[must_use]
    pub fn new(maximum_cache_bytes: usize, maximum_cache_entries: usize) -> Self {
        Self {
            cache: FactCache::new(maximum_cache_bytes, maximum_cache_entries),
            queue: VecDeque::new(),
            desired: Vec::new(),
            epoch: 0,
            stale_prepares: 0,
        }
    }

    #[must_use]
    pub const fn cache(&self) -> &FactCache {
        &self.cache
    }

    #[must_use]
    pub fn queue_len(&self) -> usize {
        self.queue.len()
    }

    #[must_use]
    pub const fn epoch(&self) -> u64 {
        self.epoch
    }

    #[must_use]
    pub const fn stale_prepares(&self) -> usize {
        self.stale_prepares
    }

    pub fn schedule_window(
        &mut self,
        document: &IndexedDocument,
        visible: Range<usize>,
        overscan: usize,
        active: Option<usize>,
        references: &ReferenceSnapshot,
    ) -> ScheduleReceipt {
        self.epoch = self.epoch.wrapping_add(1);
        let prior_queue_collapsed = self.queue.len();
        self.queue.clear();
        self.desired.clear();
        let leaf_count = document.directory().len();
        let visible_start = visible.start.min(leaf_count);
        let visible_end = visible.end.min(leaf_count).max(visible_start);

        if let Some(active) = active.filter(|ordinal| *ordinal < leaf_count) {
            self.push_desired(document, active, RetentionClass::Active);
        }
        for ordinal in visible_start..visible_end {
            self.push_desired(document, ordinal, RetentionClass::Visible);
        }
        for ordinal in visible_start.saturating_sub(overscan)..visible_start {
            self.push_desired(document, ordinal, RetentionClass::Overscan);
        }
        for ordinal in visible_end..(visible_end + overscan).min(leaf_count) {
            self.push_desired(document, ordinal, RetentionClass::Overscan);
        }

        let classes: Vec<_> = self
            .desired
            .iter()
            .map(|desired| (desired.version.id, desired.class))
            .collect();
        self.cache.set_classes(&classes);
        let mut cache_hits = 0;
        for desired in &self.desired {
            if self.cache.get(desired.version, references).is_some() {
                cache_hits += 1;
            } else {
                self.queue.push_back(ParseRequest {
                    ordinal: desired.ordinal,
                    version: desired.version,
                    revision: document.revision(),
                    epoch: self.epoch,
                    class: desired.class,
                });
            }
        }
        ScheduleReceipt {
            epoch: self.epoch,
            desired_leaves: self.desired.len(),
            cache_hits,
            queued: self.queue.len(),
            source_visible: self.queue.len(),
            prior_queue_collapsed,
        }
    }

    fn push_desired(&mut self, document: &IndexedDocument, ordinal: usize, class: RetentionClass) {
        let descriptor = document
            .directory()
            .descriptor(ordinal)
            .expect("desired ordinal is in bounds");
        if let Some(existing) = self
            .desired
            .iter_mut()
            .find(|desired| desired.version.id == descriptor.version.id)
        {
            existing.class = existing.class.max(class);
            return;
        }
        self.desired.push(DesiredLeaf {
            ordinal,
            version: descriptor.version,
            class,
        });
        self.desired
            .sort_by_key(|desired| (std::cmp::Reverse(desired.class), desired.ordinal));
    }

    pub fn prepare_next(
        &mut self,
        document: &IndexedDocument,
    ) -> Result<Option<PreparedInlineJob>, DocumentError> {
        while let Some(request) = self.queue.pop_front() {
            let Some(current) = document.directory().descriptor(request.ordinal) else {
                self.stale_prepares += 1;
                continue;
            };
            if request.revision != document.revision() || request.version != current.version {
                self.stale_prepares += 1;
                continue;
            }
            return Ok(Some(PreparedInlineJob {
                version: request.version,
                revision: request.revision,
                epoch: request.epoch,
                class: request.class,
                logical: document.leaf_source(&current)?.to_owned(),
            }));
        }
        Ok(None)
    }

    pub fn adopt(
        &mut self,
        completion: InlineCompletion,
        document: &IndexedDocument,
        references: &ReferenceSnapshot,
    ) -> Adoption {
        if completion.fragment.revision != document.revision() {
            return Adoption::StaleRevision;
        }
        let Some(current) = document.directory().descriptor_by_id(completion.version.id) else {
            return Adoption::StaleLeaf;
        };
        if current.version != completion.version {
            return Adoption::StaleLeaf;
        }
        if completion.epoch != self.epoch {
            return Adoption::StaleWindow;
        }
        let Some(desired) = self
            .desired
            .iter()
            .find(|desired| desired.version == completion.version)
        else {
            return Adoption::StaleWindow;
        };
        if !completion
            .fragment
            .reference_dependencies
            .iter()
            .all(|dependency| references.dependency_is_current(dependency))
        {
            return Adoption::StaleDependency;
        }
        match self.cache.insert(
            completion.version,
            desired.class.max(completion.class),
            completion.fragment,
        ) {
            CacheInsert::Adopted { evicted } => Adoption::Adopted { evicted },
            CacheInsert::TooLarge => Adoption::TooLarge,
        }
    }

    pub fn presentation(
        &mut self,
        descriptor: &LeafDescriptor,
        references: &ReferenceSnapshot,
    ) -> LeafPresentation {
        if let Some(fragment) = self.cache.get(descriptor.version, references) {
            LeafPresentation::Exact {
                facts: fragment.facts.len(),
                projection_facts: fragment.projection_facts.len(),
                task_list_markers: fragment
                    .facts
                    .iter()
                    .filter(|fact| fact.kind == InlineFactKind::TaskListMarker as u8)
                    .count(),
            }
        } else {
            LeafPresentation::SourceVisible {
                pending: self
                    .queue
                    .iter()
                    .any(|request| request.version == descriptor.version),
            }
        }
    }

    pub fn drain(
        &mut self,
        document: &IndexedDocument,
        references: &ReferenceSnapshot,
    ) -> Result<BatchReceipt, CacheGateError> {
        let mut receipt = BatchReceipt::default();
        while let Some(job) = self.prepare_next(document)? {
            let completion = job.run(references, document.revision())?;
            receipt.parsed_leaves += 1;
            receipt.parsed_bytes += completion.logical_bytes;
            receipt.facts += completion.fragment.facts.len();
            receipt.projection_facts += completion.fragment.projection_facts.len();
            receipt.protocol_output_bytes += completion.fragment.output_bytes();
            match self.adopt(completion, document, references) {
                Adoption::Adopted { evicted } => {
                    receipt.adopted += 1;
                    receipt.evicted += evicted;
                }
                _ => receipt.rejected += 1,
            }
        }
        Ok(receipt)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WindowProbeReceipt {
    pub leaves: usize,
    pub parsed: usize,
    pub facts: usize,
    pub cache_entries: usize,
    pub cache_bytes: usize,
    pub checksum: u64,
}

pub fn run_window_probe(leaves: usize) -> Result<WindowProbeReceipt, CacheGateError> {
    let document_bytes = leaves
        .saturating_mul(DEFAULT_LOGICAL_LEAF_BYTES + 2)
        .saturating_sub(2);
    let document = IndexedDocument::ordinary(document_bytes, DEFAULT_LOGICAL_LEAF_BYTES);
    let references = ReferenceSnapshot::default().with_symbol("label", true, "/one");
    let mut controller = LazyInlineController::new(512 * 1024, 256);
    let leaf_count = document.directory().len();
    controller.schedule_window(&document, 0..leaf_count, 0, None, &references);
    let batch = controller.drain(&document, &references)?;
    let stats = controller.cache().stats();
    Ok(WindowProbeReceipt {
        leaves: leaf_count,
        parsed: batch.parsed_leaves,
        facts: batch.facts + batch.projection_facts,
        cache_entries: stats.entries,
        cache_bytes: stats.accounted_bytes,
        checksum: (batch.protocol_output_bytes as u64)
            .wrapping_add(stats.accounted_bytes as u64)
            .wrapping_add(stats.entries as u64),
    })
}
