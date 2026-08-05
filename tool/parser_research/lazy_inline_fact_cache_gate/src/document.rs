use std::fmt;
use std::mem::size_of;
use std::ops::Range;
use std::sync::Arc;

use flark_comrak_inline_fragment_gate::InlineInputKind;
use flark_relative_output_reuse_gate::{PageId, Position};

pub const DESCRIPTORS_PER_PAGE: usize = 256;
pub const DEFAULT_LOGICAL_LEAF_BYTES: usize = 96;
pub const LEAF_SEPARATOR: &str = "\n\n";

/// Block-owned context that can affect exact inline parsing.
///
/// `ListItemParagraph` deliberately distinguishes a later paragraph from an
/// ordinary paragraph even though both use Comrak's paragraph inline entry
/// point. Only the block spine's certified first paragraph of a list item may
/// enable task-list recognition.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LeafInlineContext {
    #[default]
    Paragraph,
    ListItemParagraph {
        task_list_certified: bool,
    },
    Heading {
        level: u8,
        setext: bool,
    },
    TableCell,
}

impl LeafInlineContext {
    #[must_use]
    pub const fn parser_kind(self) -> InlineInputKind {
        match self {
            Self::Paragraph
            | Self::ListItemParagraph {
                task_list_certified: false,
            } => InlineInputKind::Paragraph,
            Self::ListItemParagraph {
                task_list_certified: true,
            } => InlineInputKind::ListItemParagraph,
            Self::Heading { level, setext } => InlineInputKind::Heading { level, setext },
            Self::TableCell => InlineInputKind::TableCell,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LeafVersion {
    pub id: PageId,
    pub content_generation: u32,
    pub inline_context: LeafInlineContext,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LeafDescriptor {
    pub ordinal: usize,
    pub version: LeafVersion,
    pub document_revision: u64,
    pub logical_range: Range<u64>,
    pub logical: Position,
    pub coverage: Position,
}

#[derive(Clone, Debug)]
struct DescriptorPage {
    first_id: u64,
    logical_bytes: Box<[u16]>,
    logical_utf16: Box<[u16]>,
    coverage_bytes: Box<[u16]>,
    coverage_utf16: Box<[u16]>,
    generations: Box<[u32]>,
    inline_contexts: Box<[LeafInlineContext]>,
    total_coverage: Position,
}

impl DescriptorPage {
    fn len(&self) -> usize {
        self.logical_bytes.len()
    }

    fn descriptor(
        &self,
        page_prefix: Position,
        local: usize,
        ordinal: usize,
        revision: u64,
    ) -> LeafDescriptor {
        let within_page = (0..local).fold(Position::default(), |prefix, index| Position {
            byte: prefix.byte + usize::from(self.coverage_bytes[index]),
            utf16: prefix.utf16 + usize::from(self.coverage_utf16[index]),
        });
        let logical = Position {
            byte: usize::from(self.logical_bytes[local]),
            utf16: usize::from(self.logical_utf16[local]),
        };
        let coverage = Position {
            byte: usize::from(self.coverage_bytes[local]),
            utf16: usize::from(self.coverage_utf16[local]),
        };
        let start = Position {
            byte: page_prefix.byte + within_page.byte,
            utf16: page_prefix.utf16 + within_page.utf16,
        };
        LeafDescriptor {
            ordinal,
            version: LeafVersion {
                id: PageId(self.first_id + local as u64),
                content_generation: self.generations[local],
                inline_context: self.inline_contexts[local],
            },
            document_revision: revision,
            logical_range: start.byte as u64..(start.byte + logical.byte) as u64,
            logical,
            coverage,
        }
    }

    fn accounted_bytes(&self) -> usize {
        size_of::<Self>()
            + self.logical_bytes.len() * size_of::<u16>()
            + self.logical_utf16.len() * size_of::<u16>()
            + self.coverage_bytes.len() * size_of::<u16>()
            + self.coverage_utf16.len() * size_of::<u16>()
            + self.generations.len() * size_of::<u32>()
            + self.inline_contexts.len() * size_of::<LeafInlineContext>()
            + 2 * size_of::<usize>()
    }
}

#[derive(Clone, Debug, Default)]
struct PrefixFenwick {
    bytes: Vec<u64>,
    utf16: Vec<u64>,
}

impl PrefixFenwick {
    fn from_pages(pages: &[Arc<DescriptorPage>]) -> Self {
        let mut result = Self {
            bytes: vec![0; pages.len() + 1],
            utf16: vec![0; pages.len() + 1],
        };
        for (index, page) in pages.iter().enumerate() {
            result.add(index, page.total_coverage);
        }
        result
    }

    fn add(&mut self, page: usize, delta: Position) {
        let mut index = page + 1;
        while index < self.bytes.len() {
            self.bytes[index] += delta.byte as u64;
            self.utf16[index] += delta.utf16 as u64;
            index += index & index.wrapping_neg();
        }
    }

    fn prefix(&self, pages: usize) -> Position {
        let mut index = pages;
        let mut bytes = 0_u64;
        let mut utf16 = 0_u64;
        while index > 0 {
            bytes += self.bytes[index];
            utf16 += self.utf16[index];
            index &= index - 1;
        }
        Position {
            byte: usize::try_from(bytes).expect("document byte length fits usize"),
            utf16: usize::try_from(utf16).expect("document UTF-16 length fits usize"),
        }
    }

    fn accounted_bytes(&self) -> usize {
        self.bytes.capacity() * size_of::<u64>() + self.utf16.capacity() * size_of::<u64>()
    }
}

/// Compact paged SoA block-leaf directory. Facts are not stored here.
#[derive(Clone, Debug)]
pub struct LeafDirectory {
    pages: Vec<Arc<DescriptorPage>>,
    prefixes: PrefixFenwick,
    leaf_count: usize,
    revision: u64,
}

impl LeafDirectory {
    #[must_use]
    pub fn uniform(target_bytes: usize, logical: Position, separator: Position) -> Self {
        assert!(logical.byte > 0 && logical.byte <= usize::from(u16::MAX));
        assert!(logical.utf16 <= usize::from(u16::MAX));
        assert!(separator.byte <= usize::from(u16::MAX));
        let stride = logical.byte + separator.byte;
        let leaf_count = if target_bytes < logical.byte {
            0
        } else {
            1 + (target_bytes - logical.byte) / stride
        };
        Self::uniform_leaves(leaf_count, logical, separator)
    }

    #[must_use]
    pub fn uniform_leaves(leaf_count: usize, logical: Position, separator: Position) -> Self {
        let mut pages = Vec::with_capacity(leaf_count.div_ceil(DESCRIPTORS_PER_PAGE));
        for page_start in (0..leaf_count).step_by(DESCRIPTORS_PER_PAGE) {
            let len = (leaf_count - page_start).min(DESCRIPTORS_PER_PAGE);
            let mut logical_bytes = Vec::with_capacity(len);
            let mut logical_utf16 = Vec::with_capacity(len);
            let mut coverage_bytes = Vec::with_capacity(len);
            let mut coverage_utf16 = Vec::with_capacity(len);
            for local in 0..len {
                let ordinal = page_start + local;
                let is_last = ordinal + 1 == leaf_count;
                logical_bytes.push(u16::try_from(logical.byte).expect("bounded logical leaf"));
                logical_utf16.push(u16::try_from(logical.utf16).expect("bounded logical leaf"));
                coverage_bytes.push(
                    u16::try_from(logical.byte + if is_last { 0 } else { separator.byte })
                        .expect("bounded coverage leaf"),
                );
                coverage_utf16.push(
                    u16::try_from(logical.utf16 + if is_last { 0 } else { separator.utf16 })
                        .expect("bounded coverage leaf"),
                );
            }
            let total_coverage = Position {
                byte: coverage_bytes.iter().map(|value| usize::from(*value)).sum(),
                utf16: coverage_utf16.iter().map(|value| usize::from(*value)).sum(),
            };
            pages.push(Arc::new(DescriptorPage {
                first_id: page_start as u64 + 1,
                logical_bytes: logical_bytes.into_boxed_slice(),
                logical_utf16: logical_utf16.into_boxed_slice(),
                coverage_bytes: coverage_bytes.into_boxed_slice(),
                coverage_utf16: coverage_utf16.into_boxed_slice(),
                generations: vec![0; len].into_boxed_slice(),
                inline_contexts: vec![LeafInlineContext::Paragraph; len].into_boxed_slice(),
                total_coverage,
            }));
        }
        let prefixes = PrefixFenwick::from_pages(&pages);
        Self {
            pages,
            prefixes,
            leaf_count,
            revision: 1,
        }
    }

    #[must_use]
    pub const fn len(&self) -> usize {
        self.leaf_count
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.leaf_count == 0
    }

    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub fn coverage(&self) -> Position {
        self.prefixes.prefix(self.pages.len())
    }

    #[must_use]
    pub fn descriptor(&self, ordinal: usize) -> Option<LeafDescriptor> {
        if ordinal >= self.leaf_count {
            return None;
        }
        let page_index = ordinal / DESCRIPTORS_PER_PAGE;
        let local = ordinal % DESCRIPTORS_PER_PAGE;
        Some(self.pages[page_index].descriptor(
            self.prefixes.prefix(page_index),
            local,
            ordinal,
            self.revision,
        ))
    }

    #[must_use]
    pub fn descriptor_by_id(&self, id: PageId) -> Option<LeafDescriptor> {
        let ordinal = usize::try_from(id.0.checked_sub(1)?).ok()?;
        self.descriptor(ordinal)
            .filter(|descriptor| descriptor.version.id == id)
    }

    pub fn bump_leaf_generation(&mut self, ordinal: usize) -> Option<LeafVersion> {
        if ordinal >= self.leaf_count {
            return None;
        }
        let page_index = ordinal / DESCRIPTORS_PER_PAGE;
        let local = ordinal % DESCRIPTORS_PER_PAGE;
        let page = Arc::make_mut(&mut self.pages[page_index]);
        page.generations[local] = page.generations[local].checked_add(1)?;
        self.revision = self.revision.checked_add(1)?;
        Some(LeafVersion {
            id: PageId(page.first_id + local as u64),
            content_generation: page.generations[local],
            inline_context: page.inline_contexts[local],
        })
    }

    pub fn set_leaf_inline_context(
        &mut self,
        ordinal: usize,
        context: LeafInlineContext,
    ) -> Option<LeafVersion> {
        if ordinal >= self.leaf_count {
            return None;
        }
        let page_index = ordinal / DESCRIPTORS_PER_PAGE;
        let local = ordinal % DESCRIPTORS_PER_PAGE;
        let page = Arc::make_mut(&mut self.pages[page_index]);
        if page.inline_contexts[local] != context {
            page.inline_contexts[local] = context;
            self.revision = self.revision.checked_add(1)?;
        }
        Some(LeafVersion {
            id: PageId(page.first_id + local as u64),
            content_generation: page.generations[local],
            inline_context: page.inline_contexts[local],
        })
    }

    pub fn advance_revision(&mut self) -> Option<u64> {
        self.revision = self.revision.checked_add(1)?;
        Some(self.revision)
    }

    /// Deterministic retained-body accounting, excluding allocator headers.
    #[must_use]
    pub fn accounted_retained_bytes(&self) -> usize {
        size_of::<Self>()
            + self.pages.capacity() * size_of::<Arc<DescriptorPage>>()
            + self
                .pages
                .iter()
                .map(|page| page.accounted_bytes())
                .sum::<usize>()
            + self.prefixes.accounted_bytes()
    }

    #[must_use]
    pub fn descriptor_page_count(&self) -> usize {
        self.pages.len()
    }

    #[must_use]
    pub fn maximum_page_leaves(&self) -> usize {
        self.pages.iter().map(|page| page.len()).max().unwrap_or(0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DocumentError {
    NoSource,
    MissingLeaf(usize),
    ReplacementMetrics {
        expected: Position,
        actual: Position,
    },
}

impl fmt::Display for DocumentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoSource => formatter.write_str("synthetic directory has no source payload"),
            Self::MissingLeaf(ordinal) => write!(formatter, "leaf {ordinal} does not exist"),
            Self::ReplacementMetrics { expected, actual } => write!(
                formatter,
                "replacement metrics {actual:?} do not match leaf metrics {expected:?}"
            ),
        }
    }
}

impl std::error::Error for DocumentError {}

#[derive(Clone, Debug)]
pub struct IndexedDocument {
    source: Option<String>,
    directory: LeafDirectory,
}

impl IndexedDocument {
    #[must_use]
    pub fn ordinary(target_bytes: usize, logical_leaf_bytes: usize) -> Self {
        assert!(logical_leaf_bytes > 0);
        let atom = "An ordinary *live* paragraph with **strong words**, `code`, and [label]. ";
        let mut leaf = String::with_capacity(logical_leaf_bytes);
        while leaf.len() + atom.len() <= logical_leaf_bytes {
            leaf.push_str(atom);
        }
        leaf.push_str(&"x".repeat(logical_leaf_bytes - leaf.len()));
        let logical = Position::of(&leaf);
        let separator = Position::of(LEAF_SEPARATOR);
        let directory = LeafDirectory::uniform(target_bytes, logical, separator);
        let mut source = String::with_capacity(directory.coverage().byte);
        for ordinal in 0..directory.len() {
            if ordinal > 0 {
                source.push_str(LEAF_SEPARATOR);
            }
            source.push_str(&leaf);
        }
        debug_assert_eq!(Position::of(&source), directory.coverage());
        Self {
            source: Some(source),
            directory,
        }
    }

    #[must_use]
    pub fn synthetic(target_bytes: usize, logical_leaf_bytes: usize) -> Self {
        let logical = Position {
            byte: logical_leaf_bytes,
            utf16: logical_leaf_bytes,
        };
        Self {
            source: None,
            directory: LeafDirectory::uniform(target_bytes, logical, Position::of(LEAF_SEPARATOR)),
        }
    }

    #[must_use]
    pub const fn directory(&self) -> &LeafDirectory {
        &self.directory
    }

    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.directory.revision()
    }

    #[must_use]
    pub fn source_len(&self) -> Option<usize> {
        self.source.as_ref().map(String::len)
    }

    pub fn leaf_source(&self, descriptor: &LeafDescriptor) -> Result<&str, DocumentError> {
        let source = self.source.as_ref().ok_or(DocumentError::NoSource)?;
        let start = usize::try_from(descriptor.logical_range.start).expect("range fits usize");
        let end = usize::try_from(descriptor.logical_range.end).expect("range fits usize");
        Ok(&source[start..end])
    }

    pub fn edit_leaf_same_metrics(
        &mut self,
        ordinal: usize,
        replacement: &str,
    ) -> Result<LeafVersion, DocumentError> {
        let descriptor = self
            .directory
            .descriptor(ordinal)
            .ok_or(DocumentError::MissingLeaf(ordinal))?;
        let actual = Position::of(replacement);
        if actual != descriptor.logical {
            return Err(DocumentError::ReplacementMetrics {
                expected: descriptor.logical,
                actual,
            });
        }
        let source = self.source.as_mut().ok_or(DocumentError::NoSource)?;
        let start = usize::try_from(descriptor.logical_range.start).expect("range fits usize");
        let end = usize::try_from(descriptor.logical_range.end).expect("range fits usize");
        source.replace_range(start..end, replacement);
        self.directory
            .bump_leaf_generation(ordinal)
            .ok_or(DocumentError::MissingLeaf(ordinal))
    }

    pub fn advance_revision(&mut self) -> Result<u64, DocumentError> {
        self.directory
            .advance_revision()
            .ok_or(DocumentError::NoSource)
    }

    pub fn set_leaf_inline_context(
        &mut self,
        ordinal: usize,
        context: LeafInlineContext,
    ) -> Result<LeafVersion, DocumentError> {
        self.directory
            .set_leaf_inline_context(ordinal, context)
            .ok_or(DocumentError::MissingLeaf(ordinal))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paged_directory_derives_absolute_positions_from_relative_metrics() {
        let directory = LeafDirectory::uniform_leaves(
            600,
            Position {
                byte: 96,
                utf16: 96,
            },
            Position { byte: 2, utf16: 2 },
        );
        assert_eq!(directory.descriptor_page_count(), 3);
        assert_eq!(directory.maximum_page_leaves(), DESCRIPTORS_PER_PAGE);
        let descriptor = directory.descriptor(300).unwrap();
        assert_eq!(descriptor.version.id, PageId(301));
        assert_eq!(descriptor.logical_range, 29_400..29_496);
    }
}
