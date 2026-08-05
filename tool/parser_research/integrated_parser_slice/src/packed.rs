//! Fixed-page packed primitives used by the integrated slice.
//!
//! These types deliberately avoid document-sized `Vec` growth. Builders own
//! one 4 KiB mutable page; sealed pages and sequence nodes are immutable.

use std::cmp::Ordering;
use std::mem::size_of;
use std::sync::Arc;

pub const PACKED_PAGE_BYTES: usize = 4096;
const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const PAGE_HASH_BASE: u64 = 0x0000_0100_0000_01b3;

fn hash_byte(hash: &mut u64, power: &mut u64, byte: u8) {
    *hash = hash
        .wrapping_mul(PAGE_HASH_BASE)
        .wrapping_add(u64::from(byte) + 1);
    *power = power.wrapping_mul(PAGE_HASH_BASE);
}

fn mix(left: u64, right: u64) -> u64 {
    left.rotate_left(17).wrapping_add(0x9e37_79b9_7f4a_7c15) ^ right.rotate_right(11)
}

fn encoded_varint_len(mut value: u64) -> usize {
    let mut len = 1;
    while value >= 0x80 {
        value >>= 7;
        len += 1;
    }
    len
}

fn encode_varint(mut value: u64, target: &mut [u8]) -> usize {
    let mut written = 0;
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        target[written] = byte;
        written += 1;
        if value == 0 {
            return written;
        }
    }
}

fn decode_varint(bytes: &[u8], cursor: &mut usize) -> Option<u64> {
    let mut value = 0u64;
    let mut shift = 0;
    while *cursor < bytes.len() && shift < 64 {
        let byte = bytes[*cursor];
        *cursor += 1;
        value |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Some(value);
        }
        shift += 7;
    }
    None
}

#[derive(Debug)]
pub struct PackedPage {
    bytes: Box<[u8]>,
    digest: u64,
    hash_power: u64,
}

impl PackedPage {
    #[must_use]
    pub fn payload(&self) -> &[u8] {
        &self.bytes
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    #[must_use]
    pub fn digest(&self) -> u64 {
        self.digest
    }

    fn hash_power(&self) -> u64 {
        self.hash_power
    }
}

#[derive(Debug)]
pub struct PackedPageBuilder {
    bytes: Box<[u8; PACKED_PAGE_BYTES]>,
    len: usize,
    digest: u64,
    hash_power: u64,
}

impl Default for PackedPageBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl PackedPageBuilder {
    #[must_use]
    pub fn new() -> Self {
        Self {
            bytes: Box::new([0; PACKED_PAGE_BYTES]),
            len: 0,
            digest: 0,
            hash_power: 1,
        }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.len
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    #[must_use]
    pub fn remaining(&self) -> usize {
        PACKED_PAGE_BYTES - self.len
    }

    pub fn try_push_byte(&mut self, byte: u8) -> bool {
        if self.len == PACKED_PAGE_BYTES {
            return false;
        }
        self.bytes[self.len] = byte;
        self.len += 1;
        hash_byte(&mut self.digest, &mut self.hash_power, byte);
        true
    }

    pub fn try_push_bytes(&mut self, bytes: &[u8]) -> bool {
        if bytes.len() > self.remaining() {
            return false;
        }
        for &byte in bytes {
            let pushed = self.try_push_byte(byte);
            debug_assert!(pushed);
        }
        true
    }

    pub fn try_push_varint(&mut self, value: u64) -> bool {
        let mut encoded = [0u8; 10];
        let len = encode_varint(value, &mut encoded);
        self.try_push_bytes(&encoded[..len])
    }

    /// Seals only the used prefix of the fixed mutable page.
    ///
    /// The bounded final copy is at most [`PACKED_PAGE_BYTES`]. Shrinking the
    /// immutable allocation prevents sparse checkpoints from retaining 4 KiB
    /// for a two-byte payload.
    #[must_use]
    pub fn seal(self) -> Arc<PackedPage> {
        Arc::new(PackedPage {
            bytes: self.bytes[..self.len].to_vec().into_boxed_slice(),
            digest: self.digest,
            hash_power: self.hash_power,
        })
    }
}

#[derive(Clone, Debug, Default)]
pub struct PackedPageSequence {
    root: Option<Arc<SequenceNode>>,
}

#[derive(Debug)]
enum SequenceNode {
    Leaf {
        page: Arc<PackedPage>,
    },
    Branch {
        left: Arc<Self>,
        right: Arc<Self>,
        height: u8,
        pages: usize,
        payload_bytes: usize,
        digest: u64,
        hash_power: u64,
    },
}

impl SequenceNode {
    fn height(&self) -> u8 {
        match self {
            Self::Leaf { .. } => 1,
            Self::Branch { height, .. } => *height,
        }
    }

    fn pages(&self) -> usize {
        match self {
            Self::Leaf { .. } => 1,
            Self::Branch { pages, .. } => *pages,
        }
    }

    fn payload_bytes(&self) -> usize {
        match self {
            Self::Leaf { page } => page.len(),
            Self::Branch { payload_bytes, .. } => *payload_bytes,
        }
    }

    fn digest(&self) -> u64 {
        match self {
            Self::Leaf { page } => page.digest(),
            Self::Branch { digest, .. } => *digest,
        }
    }

    fn hash_power(&self) -> u64 {
        match self {
            Self::Leaf { page } => page.hash_power(),
            Self::Branch { hash_power, .. } => *hash_power,
        }
    }
}

fn leaf_node(page: Arc<PackedPage>) -> Arc<SequenceNode> {
    Arc::new(SequenceNode::Leaf { page })
}

fn branch_node(left: Arc<SequenceNode>, right: Arc<SequenceNode>) -> Arc<SequenceNode> {
    Arc::new(SequenceNode::Branch {
        height: left.height().max(right.height()) + 1,
        pages: left.pages() + right.pages(),
        payload_bytes: left.payload_bytes() + right.payload_bytes(),
        digest: left
            .digest()
            .wrapping_mul(right.hash_power())
            .wrapping_add(right.digest()),
        hash_power: left.hash_power().wrapping_mul(right.hash_power()),
        left,
        right,
    })
}

fn join_nodes(left: Arc<SequenceNode>, right: Arc<SequenceNode>) -> Arc<SequenceNode> {
    let left_height = left.height();
    let right_height = right.height();
    if left_height > right_height + 1 {
        let SequenceNode::Branch {
            left: outer,
            right: inner,
            ..
        } = left.as_ref()
        else {
            unreachable!("height > 2 implies branch")
        };
        return balance_node(Arc::clone(outer), join_nodes(Arc::clone(inner), right));
    }
    if right_height > left_height + 1 {
        let SequenceNode::Branch {
            left: inner,
            right: outer,
            ..
        } = right.as_ref()
        else {
            unreachable!("height > 2 implies branch")
        };
        return balance_node(join_nodes(left, Arc::clone(inner)), Arc::clone(outer));
    }
    branch_node(left, right)
}

fn balance_node(left: Arc<SequenceNode>, right: Arc<SequenceNode>) -> Arc<SequenceNode> {
    if left.height() > right.height() + 1 {
        let SequenceNode::Branch {
            left: left_left,
            right: left_right,
            ..
        } = left.as_ref()
        else {
            unreachable!("unbalanced node is a branch")
        };
        if left_right.height() > left_left.height() {
            let SequenceNode::Branch {
                left: pivot_left,
                right: pivot_right,
                ..
            } = left_right.as_ref()
            else {
                unreachable!("inner-heavy child is a branch")
            };
            return branch_node(
                branch_node(Arc::clone(left_left), Arc::clone(pivot_left)),
                branch_node(Arc::clone(pivot_right), right),
            );
        }
        return branch_node(
            Arc::clone(left_left),
            branch_node(Arc::clone(left_right), right),
        );
    }
    if right.height() > left.height() + 1 {
        let SequenceNode::Branch {
            left: right_left,
            right: right_right,
            ..
        } = right.as_ref()
        else {
            unreachable!("unbalanced node is a branch")
        };
        if right_left.height() > right_right.height() {
            let SequenceNode::Branch {
                left: pivot_left,
                right: pivot_right,
                ..
            } = right_left.as_ref()
            else {
                unreachable!("inner-heavy child is a branch")
            };
            return branch_node(
                branch_node(left, Arc::clone(pivot_left)),
                branch_node(Arc::clone(pivot_right), Arc::clone(right_right)),
            );
        }
        return branch_node(
            branch_node(left, Arc::clone(right_left)),
            Arc::clone(right_right),
        );
    }
    branch_node(left, right)
}

fn split_nodes(
    node: &Arc<SequenceNode>,
    page_index: usize,
) -> (Option<Arc<SequenceNode>>, Option<Arc<SequenceNode>>) {
    if page_index == 0 {
        return (None, Some(Arc::clone(node)));
    }
    if page_index == node.pages() {
        return (Some(Arc::clone(node)), None);
    }
    let SequenceNode::Branch { left, right, .. } = node.as_ref() else {
        unreachable!("a leaf only permits boundary splits")
    };
    match page_index.cmp(&left.pages()) {
        Ordering::Less => {
            let (prefix, middle) = split_nodes(left, page_index);
            (prefix, concat_roots(middle, Some(Arc::clone(right))))
        }
        Ordering::Equal => (Some(Arc::clone(left)), Some(Arc::clone(right))),
        Ordering::Greater => {
            let (middle, suffix) = split_nodes(right, page_index - left.pages());
            (concat_roots(Some(Arc::clone(left)), middle), suffix)
        }
    }
}

fn concat_roots(
    left: Option<Arc<SequenceNode>>,
    right: Option<Arc<SequenceNode>>,
) -> Option<Arc<SequenceNode>> {
    match (left, right) {
        (None, other) | (other, None) => other,
        (Some(left), Some(right)) => Some(join_nodes(left, right)),
    }
}

impl PackedPageSequence {
    #[must_use]
    pub fn from_page(page: Arc<PackedPage>) -> Self {
        Self {
            root: Some(leaf_node(page)),
        }
    }

    #[must_use]
    pub fn page_count(&self) -> usize {
        self.root.as_ref().map_or(0, |root| root.pages())
    }

    #[must_use]
    pub fn payload_bytes(&self) -> usize {
        self.root.as_ref().map_or(0, |root| root.payload_bytes())
    }

    #[must_use]
    pub fn height(&self) -> usize {
        self.root.as_ref().map_or(0, |root| root.height() as usize)
    }

    #[must_use]
    pub fn digest(&self) -> u64 {
        self.root.as_ref().map_or(0, |root| root.digest())
    }

    #[must_use]
    pub fn shares_root_with(&self, other: &Self) -> bool {
        match (&self.root, &other.root) {
            (None, None) => true,
            (Some(left), Some(right)) => Arc::ptr_eq(left, right),
            _ => false,
        }
    }

    #[must_use]
    pub fn concat(&self, other: &Self) -> Self {
        Self {
            root: concat_roots(self.root.clone(), other.root.clone()),
        }
    }

    #[must_use]
    pub fn push_back(&self, page: Arc<PackedPage>) -> Self {
        self.concat(&Self::from_page(page))
    }

    /// Splits before `page_index` without visiting payload pages.
    ///
    /// # Panics
    ///
    /// Panics when `page_index` exceeds the sequence length.
    #[must_use]
    pub fn split_pages(&self, page_index: usize) -> (Self, Self) {
        assert!(page_index <= self.page_count());
        let Some(root) = &self.root else {
            return (Self::default(), Self::default());
        };
        let (left, right) = split_nodes(root, page_index);
        (Self { root: left }, Self { root: right })
    }

    #[must_use]
    pub fn pages(&self) -> PackedPageIterator {
        PackedPageIterator::new(self.root.clone())
    }

    #[must_use]
    pub fn accounted_structural_bytes(&self) -> usize {
        let pages = self.page_count();
        if pages == 0 {
            return 0;
        }
        let nodes = pages.saturating_mul(2).saturating_sub(1);
        nodes.saturating_mul(size_of::<SequenceNode>() + 2 * size_of::<usize>())
            + pages.saturating_mul(size_of::<PackedPage>() + 2 * size_of::<usize>())
    }

    #[must_use]
    pub fn allocated_sequence_nodes(&self) -> usize {
        self.page_count().saturating_mul(2).saturating_sub(1)
    }
}

#[derive(Debug)]
pub struct PackedPageIterator {
    stack: Vec<Arc<SequenceNode>>,
}

impl PackedPageIterator {
    fn new(root: Option<Arc<SequenceNode>>) -> Self {
        let mut iterator = Self { stack: Vec::new() };
        if let Some(root) = root {
            iterator.push_left(root);
        }
        iterator
    }

    fn push_left(&mut self, mut node: Arc<SequenceNode>) {
        loop {
            self.stack.push(Arc::clone(&node));
            let SequenceNode::Branch { left, .. } = node.as_ref() else {
                break;
            };
            node = Arc::clone(left);
        }
    }
}

impl Iterator for PackedPageIterator {
    type Item = Arc<PackedPage>;

    fn next(&mut self) -> Option<Self::Item> {
        let node = self.stack.pop()?;
        match node.as_ref() {
            SequenceNode::Leaf { page } => Some(Arc::clone(page)),
            SequenceNode::Branch { right, .. } => {
                self.push_left(Arc::clone(right));
                self.next()
            }
        }
    }
}

#[derive(Clone, Debug)]
struct ChainRoot {
    page: Option<Arc<ChainPage>>,
    offset: u16,
    count: u32,
    top: Option<u64>,
    digest: u64,
}

impl Default for ChainRoot {
    fn default() -> Self {
        Self {
            page: None,
            offset: 0,
            count: 0,
            top: None,
            digest: FNV_OFFSET,
        }
    }
}

#[derive(Debug)]
struct ChainPage {
    previous: ChainRoot,
    bytes: Box<[u8]>,
}

impl ChainRoot {
    fn fast_identity_eq(&self, other: &Self) -> bool {
        self.offset == other.offset
            && self.count == other.count
            && self.top == other.top
            && match (&self.page, &other.page) {
                (None, None) => true,
                (Some(left), Some(right)) => Arc::ptr_eq(left, right),
                _ => false,
            }
    }

    fn pop_value(&self) -> Option<(u64, Self)> {
        let page = self.page.as_ref()?;
        let offset = usize::from(self.offset);
        let encoded_len = usize::from(*page.bytes.get(offset.checked_sub(1)?)?);
        let start = offset.checked_sub(encoded_len + 1)?;
        let mut cursor = 0;
        let delta = decode_varint(&page.bytes[start..offset - 1], &mut cursor)?;
        let value = self.top?;
        if start == 0 {
            return Some((value, page.previous.clone()));
        }
        let next_count = self.count - 1;
        let next_top = if next_count == 0 {
            None
        } else {
            Some(value.checked_sub(delta)?)
        };
        Some((
            value,
            Self {
                page: self.page.clone(),
                offset: u16::try_from(start).ok()?,
                count: next_count,
                top: next_top,
                digest: self.digest ^ mix(value, u64::from(self.count)),
            },
        ))
    }
}

#[derive(Clone, Debug, Default)]
pub struct PackedOrdinalStackRoot {
    chain: ChainRoot,
}

impl PackedOrdinalStackRoot {
    #[must_use]
    pub fn len(&self) -> usize {
        self.chain.count as usize
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.chain.count == 0
    }

    #[must_use]
    pub fn top(&self) -> Option<u64> {
        self.chain.top
    }

    #[must_use]
    pub fn fast_identity_eq(&self, other: &Self) -> bool {
        self.chain.fast_identity_eq(&other.chain)
    }

    #[must_use]
    pub fn digest(&self) -> u64 {
        self.chain.digest
    }

    #[must_use]
    pub fn exact_comparison(&self, other: &Self) -> StackExactComparison {
        StackExactComparison {
            left: self.chain.clone(),
            right: other.chain.clone(),
            decided: if self.chain.count != other.chain.count || self.chain.top != other.chain.top {
                Some(false)
            } else if self.fast_identity_eq(other) {
                Some(true)
            } else {
                None
            },
        }
    }

    #[must_use]
    pub fn payload_bytes(&self) -> usize {
        let mut root = self.chain.clone();
        let mut bytes = 0;
        while let Some(page) = root.page {
            bytes += usize::from(root.offset);
            root = page.previous.clone();
        }
        bytes
    }

    #[must_use]
    pub fn page_count(&self) -> usize {
        let mut root = self.chain.clone();
        let mut pages = 0;
        while let Some(page) = root.page {
            pages += 1;
            root = page.previous.clone();
        }
        pages
    }

    #[must_use]
    pub fn accounted_retained_bytes(&self) -> usize {
        self.payload_bytes()
            + self
                .page_count()
                .saturating_mul(size_of::<ChainPage>() + 2 * size_of::<usize>())
    }
}

#[derive(Debug)]
pub struct PackedOrdinalStackBuilder {
    base: ChainRoot,
    bytes: Box<[u8; PACKED_PAGE_BYTES]>,
    len: usize,
    count: u32,
    top: Option<u64>,
    digest: u64,
    pages_sealed: usize,
    tail_bytes_copied: usize,
}

impl PackedOrdinalStackBuilder {
    #[must_use]
    pub fn from_root(root: &PackedOrdinalStackRoot) -> Self {
        let mut bytes = Box::new([0; PACKED_PAGE_BYTES]);
        let (base, len) = match &root.chain.page {
            Some(page) if usize::from(root.chain.offset) < PACKED_PAGE_BYTES => {
                let len = usize::from(root.chain.offset);
                bytes[..len].copy_from_slice(&page.bytes[..len]);
                (page.previous.clone(), len)
            }
            _ => (root.chain.clone(), 0),
        };
        Self {
            base,
            bytes,
            len,
            count: root.chain.count,
            top: root.chain.top,
            digest: root.chain.digest,
            pages_sealed: 0,
            tail_bytes_copied: len,
        }
    }

    /// Pushes a monotonically increasing ordinal.
    ///
    /// # Panics
    ///
    /// Panics if `value` is smaller than the current top ordinal.
    pub fn push(&mut self, value: u64) {
        let delta = self.top.map_or(value, |top| {
            value
                .checked_sub(top)
                .expect("packed ordinal stack requires monotonic pushes")
        });
        let encoded_len = encoded_varint_len(delta);
        let record_len = encoded_len + 1;
        if self.len + record_len > PACKED_PAGE_BYTES {
            self.seal_overlay();
        }
        let written = encode_varint(delta, &mut self.bytes[self.len..]);
        debug_assert_eq!(written, encoded_len);
        self.len += written;
        self.bytes[self.len] = u8::try_from(written).expect("u64 varint is at most 10 bytes");
        self.len += 1;
        self.count += 1;
        self.top = Some(value);
        self.digest ^= mix(value, u64::from(self.count));
    }

    pub fn pop(&mut self) -> Option<u64> {
        if self.len == 0 {
            let (value, root) = self.base.pop_value()?;
            self.base = root;
            self.count -= 1;
            self.top = self.base.top;
            self.digest = self.base.digest;
            return Some(value);
        }
        let encoded_len = usize::from(self.bytes[self.len - 1]);
        let start = self.len.checked_sub(encoded_len + 1)?;
        let mut cursor = 0;
        let delta = decode_varint(&self.bytes[start..self.len - 1], &mut cursor)?;
        let value = self.top?;
        self.len = start;
        self.count -= 1;
        self.top = if self.count == self.base.count {
            self.base.top
        } else {
            Some(value.checked_sub(delta)?)
        };
        self.digest ^= mix(value, u64::from(self.count) + 1);
        Some(value)
    }

    fn seal_overlay(&mut self) {
        if self.len == 0 {
            return;
        }
        let bytes = self.bytes[..self.len].to_vec().into_boxed_slice();
        let previous = self.base.clone();
        self.base = ChainRoot {
            page: Some(Arc::new(ChainPage { previous, bytes })),
            offset: u16::try_from(self.len).expect("page length is at most 4096"),
            count: self.count,
            top: self.top,
            digest: self.digest,
        };
        self.len = 0;
        self.pages_sealed += 1;
    }

    #[must_use]
    pub fn checkpoint(mut self) -> PackedOrdinalStackRoot {
        self.seal_overlay();
        PackedOrdinalStackRoot { chain: self.base }
    }

    #[must_use]
    pub fn pages_sealed(&self) -> usize {
        self.pages_sealed
    }

    #[must_use]
    pub fn tail_bytes_copied(&self) -> usize {
        self.tail_bytes_copied
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExactComparisonPoll {
    Pending,
    Equal,
    NotEqual,
}

pub struct StackExactComparison {
    left: ChainRoot,
    right: ChainRoot,
    decided: Option<bool>,
}

impl StackExactComparison {
    /// Compares at most `fuel` stack entries.
    ///
    /// # Panics
    ///
    /// Panics if `fuel` is zero.
    pub fn poll(&mut self, fuel: usize) -> ExactComparisonPoll {
        assert!(fuel > 0);
        if let Some(equal) = self.decided {
            return if equal {
                ExactComparisonPoll::Equal
            } else {
                ExactComparisonPoll::NotEqual
            };
        }
        for _ in 0..fuel {
            match (self.left.pop_value(), self.right.pop_value()) {
                (None, None) => {
                    self.decided = Some(true);
                    return ExactComparisonPoll::Equal;
                }
                (Some((left, left_root)), Some((right, right_root))) if left == right => {
                    self.left = left_root;
                    self.right = right_root;
                    if self.left.fast_identity_eq(&self.right) {
                        self.decided = Some(true);
                        return ExactComparisonPoll::Equal;
                    }
                }
                _ => {
                    self.decided = Some(false);
                    return ExactComparisonPoll::NotEqual;
                }
            }
        }
        ExactComparisonPoll::Pending
    }
}

#[derive(Debug, Default)]
pub struct PackedRecordSink {
    sealed: PackedPageSequence,
    builder: PackedPageBuilder,
    records: u64,
    pages_sealed: usize,
}

impl PackedRecordSink {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends one compact record.
    ///
    /// # Panics
    ///
    /// Panics if the record exceeds the fixed local encoding buffer or one
    /// packed page. Integrated grammar record schemas are statically bounded
    /// well below this limit.
    pub fn push(&mut self, tag: u8, fields: &[u64]) {
        const MAX_FIELDS: usize = 12;
        assert!(
            fields.len() <= MAX_FIELDS,
            "packed record has too many fields"
        );
        let mut record = [0u8; 128];
        let header = u64::from(tag) * 16 + fields.len() as u64;
        let mut len = encode_varint(header, &mut record);
        for &field in fields {
            len += encode_varint(field, &mut record[len..]);
        }
        assert!(
            len < PACKED_PAGE_BYTES,
            "one packed record must fit one page"
        );
        if self.builder.remaining() < len {
            self.seal_page();
        }
        assert!(self.builder.try_push_bytes(&record[..len]));
        self.records += 1;
    }

    fn seal_page(&mut self) {
        if self.builder.is_empty() {
            return;
        }
        let page = std::mem::take(&mut self.builder).seal();
        self.sealed = self.sealed.push_back(page);
        self.pages_sealed += 1;
    }

    #[must_use]
    pub fn finish(mut self) -> PackedRecordRoot {
        self.seal_page();
        PackedRecordRoot {
            pages: self.sealed,
            records: self.records,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct PackedRecordRoot {
    pages: PackedPageSequence,
    records: u64,
}

impl PackedRecordRoot {
    #[must_use]
    pub fn records(&self) -> u64 {
        self.records
    }

    #[must_use]
    pub fn payload_bytes(&self) -> usize {
        self.pages.payload_bytes()
    }

    #[must_use]
    pub fn pages(&self) -> &PackedPageSequence {
        &self.pages
    }

    #[must_use]
    pub fn iter(&self) -> PackedRecordIterator {
        PackedRecordIterator {
            pages: self.pages.pages(),
            page: None,
            cursor: 0,
            remaining: self.records,
        }
    }
}

impl IntoIterator for &PackedRecordRoot {
    type Item = PackedRecord;
    type IntoIter = PackedRecordIterator;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PackedRecord {
    tag: u8,
    fields: [u64; 12],
    field_count: u8,
}

impl PackedRecord {
    #[must_use]
    pub fn tag(self) -> u8 {
        self.tag
    }

    #[must_use]
    pub fn fields(&self) -> &[u64] {
        &self.fields[..usize::from(self.field_count)]
    }
}

pub struct PackedRecordIterator {
    pages: PackedPageIterator,
    page: Option<Arc<PackedPage>>,
    cursor: usize,
    remaining: u64,
}

impl Iterator for PackedRecordIterator {
    type Item = PackedRecord;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }
        loop {
            if self
                .page
                .as_ref()
                .is_some_and(|page| self.cursor < page.len())
            {
                break;
            }
            self.page = self.pages.next();
            self.cursor = 0;
            self.page.as_ref()?;
        }

        let page = self.page.as_ref()?;
        let header = decode_varint(page.payload(), &mut self.cursor)?;
        let field_count = usize::try_from(header & 0x0f).ok()?;
        if field_count > 12 {
            return None;
        }
        let tag = u8::try_from(header >> 4).ok()?;
        let mut fields = [0u64; 12];
        for field in &mut fields[..field_count] {
            *field = decode_varint(page.payload(), &mut self.cursor)?;
        }
        self.remaining -= 1;
        Some(PackedRecord {
            tag,
            fields,
            field_count: u8::try_from(field_count).ok()?,
        })
    }
}
