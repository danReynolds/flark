//! Source-driven serialized-green representation challenger.
//!
//! This is deliberately separate from the smaller Enter/Exit algebra in the
//! crate root. It tests the stronger claim: one immutable source-order rope can
//! carry generic block structure and total semantic coverage without a
//! document-wide `BlockId` directory or persistent absolute token ranks.

use std::collections::HashSet;
use std::fmt;
use std::ops::Range;
use std::sync::Arc;

use crate::{BlockId, ChildSequenceSummary, ClosedChildSummary};

pub const GREEN_PAGE_PAYLOAD_BYTES: usize = 4_096;
pub const GREEN_LEAF_HEADER_BYTES: usize = 16;
pub const GREEN_BRANCH_PAYLOAD_BYTES: usize = 96;
pub const GREEN_ARENA_SLOT_BYTES: usize = 80;
const OPEN_WITNESS_LABELS: usize = 4;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GreenMetric {
    pub bytes: u64,
    pub utf16: u64,
}

impl GreenMetric {
    fn checked_add(self, suffix: Self) -> Result<Self, GreenError> {
        Ok(Self {
            bytes: self
                .bytes
                .checked_add(suffix.bytes)
                .ok_or(GreenError::Overflow("source bytes"))?,
            utf16: self
                .utf16
                .checked_add(suffix.utf16)
                .ok_or(GreenError::Overflow("source UTF-16"))?,
        })
    }
}

/// Generic, codec-stable block kind. Kind-specific structural facts are
/// encoded as adjacent length-tagged [`GreenProperty`] tokens, so source-first
/// access does not require a document-wide `BlockId` property map.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GreenKind(pub u8);

impl GreenKind {
    pub const DOCUMENT: Self = Self(1);
    pub const BLOCK_QUOTE: Self = Self(2);
    pub const LIST: Self = Self(3);
    pub const ITEM: Self = Self(4);
    pub const PARAGRAPH: Self = Self(5);
    pub const CODE_BLOCK: Self = Self(6);
    pub const HTML_BLOCK: Self = Self(7);
    pub const TABLE: Self = Self(8);
    pub const TABLE_ROW: Self = Self(9);
    pub const TABLE_CELL: Self = Self(10);
    pub const HEADING: Self = Self(11);
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CoveragePart(pub u8);

impl CoveragePart {
    pub const CONTENT: Self = Self(1);
    pub const CONTAINER_MARKER: Self = Self(2);
    pub const BLOCK_MARKER: Self = Self(3);
    pub const GAP: Self = Self(4);
}

pub const GREEN_PROPERTY_INLINE_BYTES: usize = 16;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PropertyTag(pub u8);

impl PropertyTag {
    pub const LIST: Self = Self(1);
    pub const ITEM: Self = Self(2);
    pub const HEADING: Self = Self(3);
    pub const FENCE: Self = Self(4);
    pub const HTML: Self = Self(5);
    pub const TABLE_ALIGNMENTS: Self = Self(6);
    pub const TABLE_ALIGNMENTS_CONTINUED: Self = Self(7);
}

/// One generic property chunk immediately following its owning Enter. Larger
/// facts use typed continuation chunks; source payload itself remains in Crop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GreenProperty {
    pub tag: PropertyTag,
    len: u8,
    data: [u8; GREEN_PROPERTY_INLINE_BYTES],
}

impl GreenProperty {
    pub fn new(tag: PropertyTag, bytes: &[u8]) -> Result<Self, GreenError> {
        if tag.0 == 0 {
            return Err(GreenError::Invalid("property tag must be nonzero"));
        }
        if bytes.len() > GREEN_PROPERTY_INLINE_BYTES {
            return Err(GreenError::Invalid("property chunk exceeds inline codec"));
        }
        let mut data = [0_u8; GREEN_PROPERTY_INLINE_BYTES];
        data[..bytes.len()].copy_from_slice(bytes);
        Ok(Self {
            tag,
            len: u8::try_from(bytes.len()).map_err(|_| GreenError::Overflow("property length"))?,
            data,
        })
    }

    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.data[..usize::from(self.len)]
    }

    #[must_use]
    pub const fn packed_bytes(self) -> usize {
        // Token tag + property tag + one-byte bounded length + payload.
        3 + self.len as usize
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CoverageAtom {
    pub metric: GreenMetric,
    /// Zero owns the innermost structurally open block, one its parent, etc.
    pub owner_relative_depth: u32,
    pub part: CoveragePart,
}

impl CoverageAtom {
    pub fn new(
        bytes: u64,
        utf16: u64,
        owner_relative_depth: u32,
        part: CoveragePart,
    ) -> Result<Self, GreenError> {
        if bytes == 0 || utf16 == 0 {
            return Err(GreenError::Invalid("coverage atoms must be nonempty"));
        }
        if part.0 == 0 || part.0 > 7 {
            return Err(GreenError::Invalid("coverage part exceeds packed tag"));
        }
        Ok(Self {
            metric: GreenMetric { bytes, utf16 },
            owner_relative_depth,
            part,
        })
    }

    #[must_use]
    pub fn packed_bytes(self) -> usize {
        // One tag packs the part and whether byte and UTF-16 lengths match.
        1 + varint_bytes(u64::from(self.owner_relative_depth))
            + varint_bytes(self.metric.bytes)
            + if self.metric.bytes == self.metric.utf16 {
                0
            } else {
                varint_bytes(self.metric.utf16)
            }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GreenToken {
    Enter {
        block: BlockId,
        kind: GreenKind,
        closed: ClosedChildSummary,
    },
    Property(GreenProperty),
    Coverage(CoverageAtom),
    Exit,
}

impl GreenToken {
    pub fn enter(block: BlockId, kind: GreenKind, closed: ClosedChildSummary) -> Self {
        Self::Enter {
            block,
            kind,
            closed,
        }
    }

    #[must_use]
    pub fn packed_bytes(self) -> usize {
        match self {
            // Enter tag + one kind/fold descriptor + fixed stable ID.
            Self::Enter { .. } => 10,
            Self::Property(property) => property.packed_bytes(),
            Self::Coverage(atom) => atom.packed_bytes(),
            Self::Exit => 1,
        }
    }

    #[must_use]
    pub const fn metric(self) -> GreenMetric {
        match self {
            Self::Coverage(atom) => atom.metric,
            Self::Enter { .. } | Self::Property(_) | Self::Exit => {
                GreenMetric { bytes: 0, utf16: 0 }
            }
        }
    }
}

fn varint_bytes(mut value: u64) -> usize {
    let mut bytes = 1;
    while value >= 0x80 {
        value >>= 7;
        bytes += 1;
    }
    bytes
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct GreenSummary {
    tokens: u64,
    metric: GreenMetric,
    balance: i64,
    minimum_prefix: i64,
    minimum_enter_depth: Option<i64>,
    outermost: ChildSequenceSummary,
    /// First unmatched Enters in forward order. This fixed witness accelerates
    /// the common shallow owner/path query; it never authorizes structure and
    /// falls back to exact tree descent when depth exceeds the bound.
    first_unmatched_opens: [OpenLabel; OPEN_WITNESS_LABELS],
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct OpenLabel {
    block: BlockId,
    kind: GreenKind,
}

impl GreenSummary {
    fn token(token: GreenToken) -> Self {
        match token {
            GreenToken::Enter {
                block,
                kind,
                closed,
            } => Self {
                tokens: 1,
                metric: GreenMetric::default(),
                balance: 1,
                minimum_prefix: 0,
                minimum_enter_depth: Some(0),
                outermost: ChildSequenceSummary::singleton(closed),
                first_unmatched_opens: [
                    OpenLabel { block, kind },
                    OpenLabel::default(),
                    OpenLabel::default(),
                    OpenLabel::default(),
                ],
            },
            GreenToken::Coverage(atom) => Self {
                tokens: 1,
                metric: atom.metric,
                ..Self::default()
            },
            GreenToken::Property(_) => Self {
                tokens: 1,
                ..Self::default()
            },
            GreenToken::Exit => Self {
                tokens: 1,
                balance: -1,
                minimum_prefix: -1,
                ..Self::default()
            },
        }
    }

    fn followed_by(self, suffix: Self) -> Result<Self, GreenError> {
        let (left_opens, _) = self.unmatched()?;
        let (right_opens, right_closes) = suffix.unmatched()?;
        let remaining_left = left_opens.saturating_sub(right_closes);
        let combined_opens = remaining_left
            .checked_add(right_opens)
            .ok_or(GreenError::Overflow("unmatched-open witness"))?;
        let mut first_unmatched_opens = [OpenLabel::default(); OPEN_WITNESS_LABELS];
        let left_kept = usize::try_from(remaining_left.min(OPEN_WITNESS_LABELS as u64))
            .map_err(|_| GreenError::Overflow("open witness"))?;
        first_unmatched_opens[..left_kept]
            .copy_from_slice(&self.first_unmatched_opens[..left_kept]);
        let remaining_slots = OPEN_WITNESS_LABELS - left_kept;
        let right_kept = usize::try_from(right_opens.min(remaining_slots as u64))
            .map_err(|_| GreenError::Overflow("open witness"))?;
        first_unmatched_opens[left_kept..left_kept + right_kept]
            .copy_from_slice(&suffix.first_unmatched_opens[..right_kept]);
        let shifted_right_minimum = suffix.minimum_enter_depth.map(|depth| self.balance + depth);
        let minimum_enter_depth = match (self.minimum_enter_depth, shifted_right_minimum) {
            (None, None) => None,
            (Some(left), None) => Some(left),
            (None, Some(right)) => Some(right),
            (Some(left), Some(right)) => Some(left.min(right)),
        };
        let left_is_minimum = self.minimum_enter_depth == minimum_enter_depth;
        let right_is_minimum = shifted_right_minimum == minimum_enter_depth;
        let outermost = match (left_is_minimum, right_is_minimum) {
            (true, true) => self.outermost.followed_by(suffix.outermost),
            (true, false) => self.outermost,
            (false, true) => suffix.outermost,
            (false, false) => ChildSequenceSummary::default(),
        };
        Ok(Self {
            tokens: self
                .tokens
                .checked_add(suffix.tokens)
                .ok_or(GreenError::Overflow("token count"))?,
            metric: self.metric.checked_add(suffix.metric)?,
            balance: self
                .balance
                .checked_add(suffix.balance)
                .ok_or(GreenError::Overflow("structural depth"))?,
            minimum_prefix: self
                .minimum_prefix
                .min(self.balance + suffix.minimum_prefix),
            minimum_enter_depth,
            outermost,
            first_unmatched_opens: if combined_opens == 0 {
                [OpenLabel::default(); OPEN_WITNESS_LABELS]
            } else {
                first_unmatched_opens
            },
        })
    }

    fn from_tokens(tokens: &[GreenToken]) -> Result<Self, GreenError> {
        tokens
            .iter()
            .copied()
            .try_fold(Self::default(), |summary, token| {
                summary.followed_by(Self::token(token))
            })
    }

    fn unmatched(self) -> Result<(u64, u64), GreenError> {
        let closes = u64::try_from(self.minimum_prefix.saturating_neg())
            .map_err(|_| GreenError::Corrupt("negative unmatched-close count"))?;
        let opens = self
            .balance
            .checked_add(i64::try_from(closes).map_err(|_| GreenError::Overflow("depth"))?)
            .ok_or(GreenError::Overflow("unmatched-open count"))?;
        Ok((
            u64::try_from(opens)
                .map_err(|_| GreenError::Corrupt("negative unmatched-open count"))?,
            closes,
        ))
    }
}

fn encode_varint(mut value: u64, output: &mut Vec<u8>) {
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        output.push(byte);
        if value == 0 {
            break;
        }
    }
}

fn encode_leaf(tokens: &[GreenToken]) -> Result<Vec<u8>, GreenError> {
    let token_count =
        u16::try_from(tokens.len()).map_err(|_| GreenError::Overflow("leaf token count"))?;
    let packed_token_bytes: usize = tokens.iter().copied().map(GreenToken::packed_bytes).sum();
    let payload_len = GREEN_LEAF_HEADER_BYTES + packed_token_bytes;
    if payload_len > GREEN_PAGE_PAYLOAD_BYTES {
        return Err(GreenError::Invalid("green leaf exceeds page payload"));
    }
    let mut output = Vec::with_capacity(payload_len);
    output.resize(GREEN_LEAF_HEADER_BYTES, 0);
    output[0] = 0xa1;
    output[1] = 1;
    output[2..4].copy_from_slice(&token_count.to_le_bytes());
    output[4..8].copy_from_slice(
        &u32::try_from(packed_token_bytes)
            .map_err(|_| GreenError::Overflow("leaf payload bytes"))?
            .to_le_bytes(),
    );
    for token in tokens.iter().copied() {
        match token {
            GreenToken::Enter {
                block,
                kind,
                closed,
            } => {
                if kind.0 > 31 {
                    return Err(GreenError::Invalid("green kind exceeds packed descriptor"));
                }
                output.push(0x10);
                output.push(kind.0 | (closed.bits() << 5));
                output.extend_from_slice(&block.0.to_le_bytes());
            }
            GreenToken::Coverage(atom) => {
                let same_metric = atom.metric.bytes == atom.metric.utf16;
                output.push(0x40 | atom.part.0 | (u8::from(same_metric) << 3));
                encode_varint(u64::from(atom.owner_relative_depth), &mut output);
                encode_varint(atom.metric.bytes, &mut output);
                if !same_metric {
                    encode_varint(atom.metric.utf16, &mut output);
                }
            }
            GreenToken::Property(property) => {
                output.push(0x30);
                output.push(property.tag.0);
                output.push(property.len);
                output.extend_from_slice(property.bytes());
            }
            GreenToken::Exit => output.push(0x20),
        }
    }
    if output.len() != payload_len {
        return Err(GreenError::Corrupt("green token packed-size mismatch"));
    }
    Ok(output)
}

fn encode_branch_summary(summary: GreenSummary, height: u16) -> [u8; GREEN_BRANCH_PAYLOAD_BYTES] {
    let mut output = [0_u8; GREEN_BRANCH_PAYLOAD_BYTES];
    output[0] = 0xa2;
    output[1] = 1;
    output[2..4].copy_from_slice(&height.to_le_bytes());
    output[8..16].copy_from_slice(&summary.tokens.to_le_bytes());
    output[16..24].copy_from_slice(&summary.metric.bytes.to_le_bytes());
    output[24..32].copy_from_slice(&summary.metric.utf16.to_le_bytes());
    output[32..40].copy_from_slice(&summary.balance.to_le_bytes());
    output[40..48].copy_from_slice(&summary.minimum_prefix.to_le_bytes());
    output[48..56].copy_from_slice(
        &summary
            .minimum_enter_depth
            .unwrap_or(i64::MIN)
            .to_le_bytes(),
    );
    output[56] = summary.outermost.had_child as u8
        | ((summary.outermost.any_nonlast_child_ends_blank as u8) << 1)
        | ((summary.outermost.last_child_ends_blank as u8) << 2)
        | ((summary.outermost.list_loose_before_last as u8) << 3)
        | ((summary.outermost.last_item_loose_if_nonlast as u8) << 4)
        | ((summary.outermost.last_item_loose_if_last as u8) << 5);
    let mut cursor = 57;
    for label in summary.first_unmatched_opens {
        output[cursor..cursor + 8].copy_from_slice(&label.block.0.to_le_bytes());
        output[cursor + 8] = label.kind.0;
        cursor += 9;
    }
    output
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RouteSide {
    Left,
    Right,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GreenTokenCursor {
    root_identity: usize,
    route: Arc<[RouteSide]>,
    page_id: u64,
    local_index: u16,
}

impl GreenTokenCursor {
    #[must_use]
    pub const fn page_id(&self) -> u64 {
        self.page_id
    }

    #[must_use]
    pub fn route_depth(&self) -> usize {
        self.route.len()
    }
}

#[derive(Clone, Debug)]
enum NodeKind {
    Leaf {
        page_id: u64,
        tokens: Arc<[GreenToken]>,
        encoded: Arc<[u8]>,
    },
    Branch {
        left: Arc<Node>,
        right: Arc<Node>,
        encoded_summary: [u8; GREEN_BRANCH_PAYLOAD_BYTES],
    },
}

#[derive(Clone, Debug)]
struct Node {
    kind: NodeKind,
    summary: GreenSummary,
    height: u16,
}

type ParentRoute = Vec<(Arc<Node>, RouteSide)>;
type NodeSplit = (Option<Arc<Node>>, Option<Arc<Node>>);

struct ReverseScan<'a> {
    root_identity: usize,
    unmatched_exits: &'a mut u64,
    output: &'a mut Vec<GreenAncestor>,
    receipt: &'a mut GreenQueryReceipt,
}

impl Node {
    fn leaf(
        page_id: u64,
        tokens: Arc<[GreenToken]>,
        receipt: &mut GreenMutationReceipt,
    ) -> Result<Arc<Self>, GreenError> {
        if tokens.is_empty() {
            return Err(GreenError::Invalid("empty green leaf"));
        }
        let encoded = encode_leaf(&tokens)?;
        if encoded.len() > GREEN_PAGE_PAYLOAD_BYTES {
            return Err(GreenError::Invalid("green leaf exceeds page payload"));
        }
        let summary = GreenSummary::from_tokens(&tokens)?;
        receipt.nodes_allocated += 1;
        receipt.leaf_pages_allocated += 1;
        receipt.packed_payload_bytes_allocated += encoded.len();
        receipt.maximum_encoded_page_buffer_bytes = receipt
            .maximum_encoded_page_buffer_bytes
            .max(encoded.capacity());
        Ok(Arc::new(Self {
            kind: NodeKind::Leaf {
                page_id,
                tokens,
                encoded: Arc::from(encoded),
            },
            summary,
            height: 1,
        }))
    }

    fn branch(
        left: Arc<Self>,
        right: Arc<Self>,
        receipt: &mut GreenMutationReceipt,
    ) -> Result<Arc<Self>, GreenError> {
        let summary = left.summary.followed_by(right.summary)?;
        let height = left
            .height
            .max(right.height)
            .checked_add(1)
            .ok_or(GreenError::Overflow("tree height"))?;
        receipt.nodes_allocated += 1;
        receipt.branch_nodes_allocated += 1;
        receipt.packed_payload_bytes_allocated += GREEN_BRANCH_PAYLOAD_BYTES;
        let encoded_summary = encode_branch_summary(summary, height);
        Ok(Arc::new(Self {
            kind: NodeKind::Branch {
                left,
                right,
                encoded_summary,
            },
            summary,
            height,
        }))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GreenMutationReceipt {
    pub nodes_visited: usize,
    pub nodes_allocated: usize,
    pub leaf_pages_allocated: usize,
    pub branch_nodes_allocated: usize,
    pub packed_payload_bytes_allocated: usize,
    pub maximum_typed_page_buffer_bytes: usize,
    pub maximum_encoded_page_buffer_bytes: usize,
    pub maximum_streaming_roots: usize,
    pub maximum_streaming_bin_bytes: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GreenQueryReceipt {
    pub tree_nodes_visited: usize,
    pub leaf_tokens_scanned: usize,
    pub summary_nodes_skipped: usize,
    pub witness_fragments_used: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GreenMemoryStats {
    pub leaf_pages: usize,
    pub branch_nodes: usize,
    pub tokens: usize,
    pub semantic_blocks: usize,
    pub property_records: usize,
    pub coverage_atoms: usize,
    pub packed_token_bytes: usize,
    pub retained_payload_bytes: usize,
    pub arena_slots: usize,
    pub arena_slot_bytes: usize,
    pub accounted_retained_bytes: usize,
    /// Current Arc challenger only. Production uses the packed payload/arena
    /// model above and decodes one typed leaf on demand.
    pub prototype_typed_token_bytes: usize,
    pub prototype_heap_lower_bound: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Affinity {
    Upstream,
    Downstream,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceCoordinate {
    Bytes,
    Utf16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GreenAncestor {
    pub block: BlockId,
    pub kind: GreenKind,
    pub enter: GreenTokenCursor,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GreenSourceHit {
    pub atom: CoverageAtom,
    pub owner: BlockId,
    /// Semantic ancestry through the explicit owner.
    pub enclosing: Vec<BlockId>,
    /// Structural stack at the atom. This can be deeper than `enclosing` for
    /// an ancestor-owned continuation marker inside a paragraph hull.
    pub open_path: Vec<BlockId>,
    pub byte_range: Range<u64>,
    pub utf16_range: Range<u64>,
    pub cursor: GreenTokenCursor,
    pub ancestors: Vec<GreenAncestor>,
    pub receipt: GreenQueryReceipt,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GreenOwnerHit {
    pub atom: CoverageAtom,
    pub owner: BlockId,
    pub enclosing: Vec<BlockId>,
    pub open_path: Vec<BlockId>,
    pub byte_range: Range<u64>,
    pub utf16_range: Range<u64>,
    pub cursor: GreenTokenCursor,
    pub receipt: GreenQueryReceipt,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GreenViewportReceipt {
    pub tree_nodes_visited: usize,
    pub leaf_pages_visited: usize,
    pub leaf_tokens_scanned: usize,
    pub coverage_atoms: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GreenCoverageWindow {
    pub owners: Vec<(BlockId, CoveragePart)>,
    pub receipt: GreenViewportReceipt,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GreenBlockSpan {
    pub block: BlockId,
    pub kind: GreenKind,
    pub byte_range: Range<u64>,
    pub utf16_range: Range<u64>,
    pub enter: GreenTokenCursor,
    pub exit: GreenTokenCursor,
    pub receipt: GreenQueryReceipt,
}

#[derive(Clone, Debug)]
pub struct SerializedGreenSequence {
    root: Option<Arc<Node>>,
    next_page_id: u64,
}

impl SerializedGreenSequence {
    pub fn from_tokens(
        tokens: impl IntoIterator<Item = GreenToken>,
        receipt: &mut GreenMutationReceipt,
    ) -> Result<Self, GreenError> {
        Self::from_tokens_starting_at(tokens, 1, receipt)
    }

    fn from_tokens_starting_at(
        tokens: impl IntoIterator<Item = GreenToken>,
        mut next_page_id: u64,
        receipt: &mut GreenMutationReceipt,
    ) -> Result<Self, GreenError> {
        let mut page = Vec::<GreenToken>::new();
        let mut page_bytes = GREEN_LEAF_HEADER_BYTES;
        let mut bins = Vec::<Option<Arc<Node>>>::new();
        for token in tokens {
            let token_bytes = token.packed_bytes();
            if token_bytes + GREEN_LEAF_HEADER_BYTES > GREEN_PAGE_PAYLOAD_BYTES {
                return Err(GreenError::Invalid("one token exceeds green page"));
            }
            if !page.is_empty() && page_bytes + token_bytes > GREEN_PAGE_PAYLOAD_BYTES {
                let leaf = Node::leaf(next_page_id, Arc::from(page.as_slice()), receipt)?;
                next_page_id += 1;
                push_streaming_root(leaf, &mut bins, receipt)?;
                page.clear();
                page_bytes = GREEN_LEAF_HEADER_BYTES;
            }
            page.push(token);
            page_bytes += token_bytes;
            receipt.maximum_typed_page_buffer_bytes = receipt
                .maximum_typed_page_buffer_bytes
                .max(page.capacity() * std::mem::size_of::<GreenToken>());
            receipt.maximum_encoded_page_buffer_bytes =
                receipt.maximum_encoded_page_buffer_bytes.max(page_bytes);
        }
        if !page.is_empty() {
            let leaf = Node::leaf(next_page_id, Arc::from(page.as_slice()), receipt)?;
            next_page_id += 1;
            push_streaming_root(leaf, &mut bins, receipt)?;
        }
        let mut root = None;
        for candidate in bins.into_iter().rev().flatten() {
            root = Some(match root {
                None => candidate,
                Some(prefix) => join(prefix, candidate, receipt)?,
            });
        }
        Ok(Self { root, next_page_id })
    }

    #[must_use]
    pub fn metric(&self) -> GreenMetric {
        self.root
            .as_ref()
            .map_or_else(GreenMetric::default, |root| root.summary.metric)
    }

    #[must_use]
    pub fn root_identity(&self) -> usize {
        self.root
            .as_ref()
            .map_or(0, |root| Arc::as_ptr(root) as usize)
    }

    pub fn source_lookup(
        &self,
        coordinate: SourceCoordinate,
        offset: u64,
        affinity: Affinity,
    ) -> Result<GreenSourceHit, GreenError> {
        let root = self.root.as_ref().ok_or(GreenError::SourceOutOfBounds)?;
        let total = metric_axis(root.summary.metric, coordinate);
        if offset > total || total == 0 {
            return Err(GreenError::SourceOutOfBounds);
        }
        let probe = match affinity {
            Affinity::Upstream if offset > 0 => offset - 1,
            Affinity::Downstream if offset == total => total - 1,
            Affinity::Upstream | Affinity::Downstream => offset,
        };
        let mut route = Vec::new();
        let mut prefix = GreenMetric::default();
        let mut receipt = GreenQueryReceipt::default();
        let (atom, cursor, byte_range, utf16_range) = locate_source(
            root,
            coordinate,
            probe,
            &mut prefix,
            &mut route,
            self.root_identity(),
            &mut receipt,
        )?;
        let mut ancestors_inner_first = Vec::new();
        self.collect_ancestors_before(&cursor, &mut ancestors_inner_first, &mut receipt)?;
        ancestors_inner_first.reverse();
        let ancestors = ancestors_inner_first;
        let owner_depth = usize::try_from(atom.owner_relative_depth)
            .map_err(|_| GreenError::Overflow("coverage owner depth"))?;
        if owner_depth >= ancestors.len() {
            return Err(GreenError::Invalid(
                "coverage owner depth escapes open path",
            ));
        }
        let owner_index = ancestors.len() - 1 - owner_depth;
        let owner = ancestors[owner_index].block;
        Ok(GreenSourceHit {
            atom,
            owner,
            enclosing: ancestors[..=owner_index]
                .iter()
                .map(|ancestor| ancestor.block)
                .collect(),
            open_path: ancestors.iter().map(|ancestor| ancestor.block).collect(),
            byte_range,
            utf16_range,
            cursor,
            ancestors,
            receipt,
        })
    }

    /// Fast path for viewport discovery. A fixed-size unmatched-Enter witness
    /// recovers shallow ancestry without retaining a depth vector or page/ID
    /// directory. Deep paths fall back to the exact reverse descent used by
    /// `source_lookup`.
    pub fn source_owner_lookup(
        &self,
        coordinate: SourceCoordinate,
        offset: u64,
        affinity: Affinity,
    ) -> Result<GreenOwnerHit, GreenError> {
        let root = self.root.as_ref().ok_or(GreenError::SourceOutOfBounds)?;
        let total = metric_axis(root.summary.metric, coordinate);
        if offset > total || total == 0 {
            return Err(GreenError::SourceOutOfBounds);
        }
        let probe = match affinity {
            Affinity::Upstream if offset > 0 => offset - 1,
            Affinity::Downstream if offset == total => total - 1,
            Affinity::Upstream | Affinity::Downstream => offset,
        };
        let mut route = Vec::new();
        let mut prefix = GreenMetric::default();
        let mut receipt = GreenQueryReceipt::default();
        let (atom, cursor, byte_range, utf16_range) = locate_source(
            root,
            coordinate,
            probe,
            &mut prefix,
            &mut route,
            self.root_identity(),
            &mut receipt,
        )?;
        let mut labels_inner_first = Vec::new();
        self.collect_open_labels_before(&cursor, &mut labels_inner_first, &mut receipt)?;
        labels_inner_first.reverse();
        let owner_depth = usize::try_from(atom.owner_relative_depth)
            .map_err(|_| GreenError::Overflow("coverage owner depth"))?;
        if owner_depth >= labels_inner_first.len() {
            return Err(GreenError::Invalid(
                "coverage owner depth escapes open path",
            ));
        }
        let owner_index = labels_inner_first.len() - 1 - owner_depth;
        Ok(GreenOwnerHit {
            atom,
            owner: labels_inner_first[owner_index].block,
            enclosing: labels_inner_first[..=owner_index]
                .iter()
                .map(|label| label.block)
                .collect(),
            open_path: labels_inner_first.iter().map(|label| label.block).collect(),
            byte_range,
            utf16_range,
            cursor,
            receipt,
        })
    }

    pub fn coverage_window(
        &self,
        start: &GreenOwnerHit,
        maximum_atoms: usize,
    ) -> Result<GreenCoverageWindow, GreenError> {
        self.validate_cursor(&start.cursor)?;
        let mut stack = start.open_path.clone();
        let mut route = start.cursor.route.to_vec();
        let mut local = usize::from(start.cursor.local_index);
        let mut owners = Vec::with_capacity(maximum_atoms);
        let mut receipt = GreenViewportReceipt::default();
        loop {
            let node = self.node_at_route(&route)?;
            receipt.tree_nodes_visited += route.len() + 1;
            receipt.leaf_pages_visited += 1;
            let NodeKind::Leaf { tokens, .. } = &node.kind else {
                return Err(GreenError::Corrupt("viewport route is not a leaf"));
            };
            for token in tokens[local..].iter().copied() {
                receipt.leaf_tokens_scanned += 1;
                match token {
                    GreenToken::Enter { block, .. } => stack.push(block),
                    GreenToken::Exit => {
                        stack
                            .pop()
                            .ok_or(GreenError::Corrupt("viewport stack underflow"))?;
                    }
                    GreenToken::Property(_) => {}
                    GreenToken::Coverage(atom) => {
                        let depth = usize::try_from(atom.owner_relative_depth)
                            .map_err(|_| GreenError::Overflow("coverage owner depth"))?;
                        if depth >= stack.len() {
                            return Err(GreenError::Invalid(
                                "coverage owner depth escapes open path",
                            ));
                        }
                        owners.push((stack[stack.len() - 1 - depth], atom.part));
                        receipt.coverage_atoms += 1;
                        if owners.len() == maximum_atoms {
                            return Ok(GreenCoverageWindow { owners, receipt });
                        }
                    }
                }
            }
            let Some(next_route) = self.next_leaf_route(&route)? else {
                return Ok(GreenCoverageWindow { owners, receipt });
            };
            route = next_route;
            local = 0;
        }
    }

    pub fn block_span_from_hit(
        &self,
        hit: &GreenSourceHit,
        enclosing_index: usize,
    ) -> Result<GreenBlockSpan, GreenError> {
        let ancestor = hit
            .ancestors
            .get(enclosing_index)
            .ok_or(GreenError::Invalid("enclosing index out of bounds"))?;
        let mut receipt = GreenQueryReceipt::default();
        let exit = self.matching_exit(&ancestor.enter, &mut receipt)?;
        let start = self.prefix_metric(&ancestor.enter, &mut receipt)?;
        let end = self.prefix_metric(&exit, &mut receipt)?;
        Ok(GreenBlockSpan {
            block: ancestor.block,
            kind: ancestor.kind,
            byte_range: start.bytes..end.bytes,
            utf16_range: start.utf16..end.utf16,
            enter: ancestor.enter.clone(),
            exit,
            receipt,
        })
    }

    pub fn properties_from_hit(
        &self,
        hit: &GreenSourceHit,
        enclosing_index: usize,
    ) -> Result<Vec<GreenProperty>, GreenError> {
        let ancestor = hit
            .ancestors
            .get(enclosing_index)
            .ok_or(GreenError::Invalid("enclosing index out of bounds"))?;
        let mut cursor = ancestor.enter.clone();
        let mut output = Vec::new();
        while let Some(next) = self.next_cursor(&cursor)? {
            match self.token_at_cursor(&next)? {
                GreenToken::Property(property) => {
                    output.push(property);
                    cursor = next;
                }
                _ => break,
            }
        }
        Ok(output)
    }

    pub fn direct_child_summary(
        &self,
        span: &GreenBlockSpan,
        receipt: &mut GreenQueryReceipt,
    ) -> Result<ChildSequenceSummary, GreenError> {
        self.validate_cursor(&span.enter)?;
        self.validate_cursor(&span.exit)?;
        let summary = self.summary_between(&span.enter, &span.exit, receipt)?;
        if summary.balance != 0 || summary.minimum_prefix < 0 {
            return Err(GreenError::Corrupt("container interior is not balanced"));
        }
        if summary.minimum_enter_depth.is_some_and(|depth| depth != 0) {
            return Err(GreenError::Corrupt("container child depth is invalid"));
        }
        Ok(summary.outermost)
    }

    pub fn subtree_blocks(&self, span: &GreenBlockSpan) -> Result<Vec<BlockId>, GreenError> {
        self.validate_cursor(&span.enter)?;
        self.validate_cursor(&span.exit)?;
        let mut output = Vec::new();
        let mut cursor = span.enter.clone();
        loop {
            if let GreenToken::Enter { block, .. } = self.token_at_cursor(&cursor)? {
                output.push(block);
            }
            if cursor == span.exit {
                break;
            }
            cursor = self
                .next_cursor(&cursor)?
                .ok_or(GreenError::Corrupt("subtree escaped document"))?;
        }
        Ok(output)
    }

    /// A candidate splice begins from a cursor returned by current-root source
    /// descent. The route is ephemeral and revision-scoped; an old route fails
    /// closed after adoption even when its leaf page survives unchanged.
    pub fn splice_before(
        &self,
        cursor: &GreenTokenCursor,
        replacement: impl IntoIterator<Item = GreenToken>,
        receipt: &mut GreenMutationReceipt,
    ) -> Result<Self, GreenError> {
        self.validate_cursor(cursor)?;
        let root = self.root.as_ref().ok_or(GreenError::StaleCursor)?;
        let mut next_page_id = self.next_page_id;
        let (left, right) = split_at_route(
            root.clone(),
            &cursor.route,
            usize::from(cursor.local_index),
            &mut next_page_id,
            receipt,
        )?;
        let replacement = Self::from_tokens_starting_at(replacement, next_page_id, receipt)?;
        next_page_id = replacement.next_page_id;
        let root = join_optional(
            join_optional(left, replacement.root, receipt)?,
            right,
            receipt,
        )?;
        Ok(Self { root, next_page_id })
    }

    pub fn replace_token(
        &self,
        cursor: &GreenTokenCursor,
        replacement: GreenToken,
        receipt: &mut GreenMutationReceipt,
    ) -> Result<Self, GreenError> {
        self.validate_cursor(cursor)?;
        let root = replace_at_route(
            self.root.as_ref().ok_or(GreenError::StaleCursor)?.clone(),
            &cursor.route,
            usize::from(cursor.local_index),
            replacement,
            self.next_page_id,
            receipt,
        )?;
        Ok(Self {
            root: Some(root),
            next_page_id: self.next_page_id + 1,
        })
    }

    pub fn validate_cursor(&self, cursor: &GreenTokenCursor) -> Result<(), GreenError> {
        if cursor.root_identity != self.root_identity() {
            return Err(GreenError::StaleCursor);
        }
        let (leaf, _) = self.resolve_cursor(cursor)?;
        match &leaf.kind {
            NodeKind::Leaf {
                page_id, tokens, ..
            } if *page_id == cursor.page_id && usize::from(cursor.local_index) < tokens.len() => {
                Ok(())
            }
            NodeKind::Leaf { .. } => Err(GreenError::StaleCursor),
            NodeKind::Branch { .. } => Err(GreenError::Corrupt("cursor resolved to branch")),
        }
    }

    #[must_use]
    pub fn memory_stats(&self) -> GreenMemoryStats {
        Self::shared_memory_stats(&[self])
    }

    #[must_use]
    pub fn shared_memory_stats(sequences: &[&Self]) -> GreenMemoryStats {
        let mut stats = GreenMemoryStats::default();
        let roots = sequences
            .iter()
            .filter_map(|sequence| sequence.root.clone())
            .collect::<Vec<_>>();
        collect_shared_memory(roots, &mut HashSet::new(), &mut stats);
        finalize_memory_stats(&mut stats);
        stats
    }

    fn collect_ancestors_before(
        &self,
        cursor: &GreenTokenCursor,
        output: &mut Vec<GreenAncestor>,
        receipt: &mut GreenQueryReceipt,
    ) -> Result<(), GreenError> {
        let (leaf, parents) = self.resolve_cursor(cursor)?;
        let mut unmatched_exits = 0_u64;
        let NodeKind::Leaf { tokens, .. } = &leaf.kind else {
            return Err(GreenError::Corrupt("cursor leaf is branch"));
        };
        let mut route = cursor.route.to_vec();
        let mut scan = ReverseScan {
            root_identity: self.root_identity(),
            unmatched_exits: &mut unmatched_exits,
            output,
            receipt,
        };
        scan_page_reverse(
            tokens,
            0..usize::from(cursor.local_index),
            &route,
            cursor.page_id,
            &mut scan,
        )?;
        for (parent, side) in parents.into_iter().rev() {
            route.pop();
            if side == RouteSide::Right {
                let NodeKind::Branch { left, .. } = &parent.kind else {
                    return Err(GreenError::Corrupt("cursor parent is leaf"));
                };
                route.push(RouteSide::Left);
                scan_node_reverse(left, &mut route, &mut scan)?;
                route.pop();
            }
        }
        if *scan.unmatched_exits != 0 {
            return Err(GreenError::Corrupt("coverage appears after unmatched Exit"));
        }
        Ok(())
    }

    fn collect_open_labels_before(
        &self,
        cursor: &GreenTokenCursor,
        output: &mut Vec<OpenLabel>,
        receipt: &mut GreenQueryReceipt,
    ) -> Result<(), GreenError> {
        let (leaf, parents) = self.resolve_cursor(cursor)?;
        let NodeKind::Leaf { tokens, .. } = &leaf.kind else {
            return Err(GreenError::Corrupt("cursor leaf is branch"));
        };
        let mut unmatched_exits = 0_u64;
        let prefix = GreenSummary::from_tokens(&tokens[..usize::from(cursor.local_index)])?;
        use_open_witness_or_scan(
            prefix,
            Some((&tokens[..usize::from(cursor.local_index)], cursor.page_id)),
            &mut unmatched_exits,
            output,
            receipt,
        )?;
        for (parent, side) in parents.into_iter().rev() {
            if side == RouteSide::Right {
                let NodeKind::Branch { left, .. } = &parent.kind else {
                    return Err(GreenError::Corrupt("cursor parent is leaf"));
                };
                scan_labels_reverse_node(left, &mut unmatched_exits, output, receipt)?;
            }
        }
        if unmatched_exits != 0 {
            return Err(GreenError::Corrupt("coverage appears after unmatched Exit"));
        }
        Ok(())
    }

    fn matching_exit(
        &self,
        enter: &GreenTokenCursor,
        receipt: &mut GreenQueryReceipt,
    ) -> Result<GreenTokenCursor, GreenError> {
        let (leaf, parents) = self.resolve_cursor(enter)?;
        let NodeKind::Leaf { tokens, .. } = &leaf.kind else {
            return Err(GreenError::Corrupt("cursor leaf is branch"));
        };
        if !matches!(
            tokens.get(usize::from(enter.local_index)),
            Some(GreenToken::Enter { .. })
        ) {
            return Err(GreenError::Invalid("matching Exit requires Enter cursor"));
        }
        let mut depth = 1_i64;
        let mut route = enter.route.to_vec();
        if let Some(found) = scan_page_forward_for_exit(
            tokens,
            usize::from(enter.local_index) + 1..tokens.len(),
            &route,
            self.root_identity(),
            enter.page_id,
            &mut depth,
            receipt,
        )? {
            return Ok(found);
        }
        for (parent, side) in parents.into_iter().rev() {
            route.pop();
            if side == RouteSide::Left {
                let NodeKind::Branch { right, .. } = &parent.kind else {
                    return Err(GreenError::Corrupt("cursor parent is leaf"));
                };
                route.push(RouteSide::Right);
                if let Some(found) =
                    find_exit_in_node(right, &mut route, self.root_identity(), &mut depth, receipt)?
                {
                    return Ok(found);
                }
                route.pop();
            }
        }
        Err(GreenError::Corrupt("unclosed Enter"))
    }

    fn prefix_metric(
        &self,
        cursor: &GreenTokenCursor,
        receipt: &mut GreenQueryReceipt,
    ) -> Result<GreenMetric, GreenError> {
        self.validate_cursor(cursor)?;
        let mut node = self.root.as_ref().ok_or(GreenError::StaleCursor)?.clone();
        let mut prefix = GreenMetric::default();
        for side in cursor.route.iter().copied() {
            receipt.tree_nodes_visited += 1;
            let NodeKind::Branch { left, right, .. } = &node.kind else {
                return Err(GreenError::StaleCursor);
            };
            match side {
                RouteSide::Left => node = left.clone(),
                RouteSide::Right => {
                    prefix = prefix.checked_add(left.summary.metric)?;
                    node = right.clone();
                }
            }
        }
        receipt.tree_nodes_visited += 1;
        let NodeKind::Leaf { tokens, .. } = &node.kind else {
            return Err(GreenError::StaleCursor);
        };
        for token in &tokens[..usize::from(cursor.local_index)] {
            receipt.leaf_tokens_scanned += 1;
            prefix = prefix.checked_add(token.metric())?;
        }
        Ok(prefix)
    }

    fn summary_between(
        &self,
        start: &GreenTokenCursor,
        end: &GreenTokenCursor,
        receipt: &mut GreenQueryReceipt,
    ) -> Result<GreenSummary, GreenError> {
        let (start_leaf, parents) = self.resolve_cursor(start)?;
        self.validate_cursor(end)?;
        let NodeKind::Leaf { tokens, .. } = &start_leaf.kind else {
            return Err(GreenError::StaleCursor);
        };
        if start.route == end.route {
            let start_local = usize::from(start.local_index) + 1;
            let end_local = usize::from(end.local_index);
            if start_local > end_local || end_local > tokens.len() {
                return Err(GreenError::Invalid("reversed token cursor interval"));
            }
            receipt.leaf_tokens_scanned += end_local - start_local;
            return GreenSummary::from_tokens(&tokens[start_local..end_local]);
        }

        let start_local = usize::from(start.local_index) + 1;
        receipt.leaf_tokens_scanned += tokens.len() - start_local;
        let mut summary = GreenSummary::from_tokens(&tokens[start_local..])?;
        let mut route = start.route.to_vec();
        for (parent, side) in parents.into_iter().rev() {
            route.pop();
            if side == RouteSide::Left {
                let NodeKind::Branch { right, .. } = &parent.kind else {
                    return Err(GreenError::Corrupt("interval parent is leaf"));
                };
                route.push(RouteSide::Right);
                if end.route.starts_with(&route) {
                    let suffix = summary_before_cursor(
                        right,
                        &end.route[route.len()..],
                        usize::from(end.local_index),
                        receipt,
                    )?;
                    return summary.followed_by(suffix);
                }
                receipt.summary_nodes_skipped += 1;
                summary = summary.followed_by(right.summary)?;
                route.pop();
            }
        }
        Err(GreenError::Invalid(
            "end cursor does not follow start cursor",
        ))
    }

    fn resolve_cursor(
        &self,
        cursor: &GreenTokenCursor,
    ) -> Result<(Arc<Node>, ParentRoute), GreenError> {
        if cursor.root_identity != self.root_identity() {
            return Err(GreenError::StaleCursor);
        }
        let mut node = self.root.as_ref().ok_or(GreenError::StaleCursor)?.clone();
        let mut parents = Vec::with_capacity(cursor.route.len());
        for side in cursor.route.iter().copied() {
            let NodeKind::Branch { left, right, .. } = &node.kind else {
                return Err(GreenError::StaleCursor);
            };
            parents.push((node.clone(), side));
            node = match side {
                RouteSide::Left => left.clone(),
                RouteSide::Right => right.clone(),
            };
        }
        Ok((node, parents))
    }

    fn token_at_cursor(&self, cursor: &GreenTokenCursor) -> Result<GreenToken, GreenError> {
        let (leaf, _) = self.resolve_cursor(cursor)?;
        let NodeKind::Leaf {
            page_id, tokens, ..
        } = &leaf.kind
        else {
            return Err(GreenError::StaleCursor);
        };
        if *page_id != cursor.page_id {
            return Err(GreenError::StaleCursor);
        }
        tokens
            .get(usize::from(cursor.local_index))
            .copied()
            .ok_or(GreenError::StaleCursor)
    }

    fn next_cursor(
        &self,
        cursor: &GreenTokenCursor,
    ) -> Result<Option<GreenTokenCursor>, GreenError> {
        let (leaf, _) = self.resolve_cursor(cursor)?;
        let NodeKind::Leaf {
            page_id, tokens, ..
        } = &leaf.kind
        else {
            return Err(GreenError::StaleCursor);
        };
        let next_local = usize::from(cursor.local_index) + 1;
        if next_local < tokens.len() {
            return Ok(Some(GreenTokenCursor {
                root_identity: self.root_identity(),
                route: cursor.route.clone(),
                page_id: *page_id,
                local_index: u16::try_from(next_local)
                    .map_err(|_| GreenError::Overflow("leaf token index"))?,
            }));
        }

        let mut route = cursor.route.to_vec();
        while let Some(side) = route.pop() {
            if side == RouteSide::Left {
                route.push(RouteSide::Right);
                let mut node = self.node_at_route(&route)?;
                while let NodeKind::Branch { left, .. } = &node.kind {
                    route.push(RouteSide::Left);
                    node = left.clone();
                }
                let NodeKind::Leaf {
                    page_id, tokens, ..
                } = &node.kind
                else {
                    return Err(GreenError::Corrupt("successor is not leaf"));
                };
                if tokens.is_empty() {
                    return Err(GreenError::Corrupt("empty successor leaf"));
                }
                return Ok(Some(GreenTokenCursor {
                    root_identity: self.root_identity(),
                    route: Arc::from(route),
                    page_id: *page_id,
                    local_index: 0,
                }));
            }
        }
        Ok(None)
    }

    fn node_at_route(&self, route: &[RouteSide]) -> Result<Arc<Node>, GreenError> {
        let mut node = self.root.as_ref().ok_or(GreenError::StaleCursor)?.clone();
        for side in route {
            let NodeKind::Branch { left, right, .. } = &node.kind else {
                return Err(GreenError::StaleCursor);
            };
            node = match side {
                RouteSide::Left => left.clone(),
                RouteSide::Right => right.clone(),
            };
        }
        Ok(node)
    }

    fn next_leaf_route(&self, current: &[RouteSide]) -> Result<Option<Vec<RouteSide>>, GreenError> {
        let mut route = current.to_vec();
        while let Some(side) = route.pop() {
            if side == RouteSide::Left {
                route.push(RouteSide::Right);
                let mut node = self.node_at_route(&route)?;
                while let NodeKind::Branch { left, .. } = &node.kind {
                    route.push(RouteSide::Left);
                    node = left.clone();
                }
                return Ok(Some(route));
            }
        }
        Ok(None)
    }
}

fn metric_axis(metric: GreenMetric, coordinate: SourceCoordinate) -> u64 {
    match coordinate {
        SourceCoordinate::Bytes => metric.bytes,
        SourceCoordinate::Utf16 => metric.utf16,
    }
}

#[allow(clippy::too_many_arguments)]
fn locate_source(
    node: &Arc<Node>,
    coordinate: SourceCoordinate,
    mut probe: u64,
    prefix: &mut GreenMetric,
    route: &mut Vec<RouteSide>,
    root_identity: usize,
    receipt: &mut GreenQueryReceipt,
) -> Result<(CoverageAtom, GreenTokenCursor, Range<u64>, Range<u64>), GreenError> {
    receipt.tree_nodes_visited += 1;
    match &node.kind {
        NodeKind::Branch { left, right, .. } => {
            let left_axis = metric_axis(left.summary.metric, coordinate);
            if probe < left_axis {
                route.push(RouteSide::Left);
                locate_source(
                    left,
                    coordinate,
                    probe,
                    prefix,
                    route,
                    root_identity,
                    receipt,
                )
            } else {
                probe -= left_axis;
                *prefix = prefix.checked_add(left.summary.metric)?;
                route.push(RouteSide::Right);
                locate_source(
                    right,
                    coordinate,
                    probe,
                    prefix,
                    route,
                    root_identity,
                    receipt,
                )
            }
        }
        NodeKind::Leaf {
            page_id, tokens, ..
        } => {
            let mut local_prefix = *prefix;
            for (local, token) in tokens.iter().copied().enumerate() {
                receipt.leaf_tokens_scanned += 1;
                let metric = token.metric();
                let length = metric_axis(metric, coordinate);
                if let GreenToken::Coverage(atom) = token {
                    if probe < length {
                        let cursor = GreenTokenCursor {
                            root_identity,
                            route: Arc::from(route.as_slice()),
                            page_id: *page_id,
                            local_index: u16::try_from(local)
                                .map_err(|_| GreenError::Overflow("leaf token index"))?,
                        };
                        return Ok((
                            atom,
                            cursor,
                            local_prefix.bytes..local_prefix.bytes + atom.metric.bytes,
                            local_prefix.utf16..local_prefix.utf16 + atom.metric.utf16,
                        ));
                    }
                    probe -= length;
                }
                local_prefix = local_prefix.checked_add(metric)?;
            }
            Err(GreenError::Corrupt("source metric escaped selected leaf"))
        }
    }
}

fn use_open_witness_or_scan(
    summary: GreenSummary,
    page: Option<(&[GreenToken], u64)>,
    unmatched_exits: &mut u64,
    output: &mut Vec<OpenLabel>,
    receipt: &mut GreenQueryReceipt,
) -> Result<bool, GreenError> {
    let (opens, closes) = summary.unmatched()?;
    if opens <= *unmatched_exits {
        *unmatched_exits = *unmatched_exits - opens + closes;
        receipt.summary_nodes_skipped += 1;
        return Ok(true);
    }
    let labels = opens - *unmatched_exits;
    if labels <= OPEN_WITNESS_LABELS as u64 {
        let labels = usize::try_from(labels).map_err(|_| GreenError::Overflow("open witness"))?;
        output.extend(
            summary.first_unmatched_opens[..labels]
                .iter()
                .rev()
                .copied(),
        );
        *unmatched_exits = closes;
        receipt.witness_fragments_used += 1;
        return Ok(true);
    }
    let Some((tokens, _page_id)) = page else {
        return Ok(false);
    };
    for token in tokens.iter().rev().copied() {
        receipt.leaf_tokens_scanned += 1;
        match token {
            GreenToken::Exit => *unmatched_exits += 1,
            GreenToken::Enter { block, kind, .. } if *unmatched_exits == 0 => {
                output.push(OpenLabel { block, kind });
            }
            GreenToken::Enter { .. } => *unmatched_exits -= 1,
            GreenToken::Property(_) | GreenToken::Coverage(_) => {}
        }
    }
    Ok(true)
}

fn scan_labels_reverse_node(
    node: &Arc<Node>,
    unmatched_exits: &mut u64,
    output: &mut Vec<OpenLabel>,
    receipt: &mut GreenQueryReceipt,
) -> Result<(), GreenError> {
    receipt.tree_nodes_visited += 1;
    let page = match &node.kind {
        NodeKind::Leaf {
            page_id, tokens, ..
        } => Some((tokens.as_ref(), *page_id)),
        NodeKind::Branch { .. } => None,
    };
    if use_open_witness_or_scan(node.summary, page, unmatched_exits, output, receipt)? {
        return Ok(());
    }
    let NodeKind::Branch { left, right, .. } = &node.kind else {
        return Err(GreenError::Corrupt("leaf witness fallback failed"));
    };
    scan_labels_reverse_node(right, unmatched_exits, output, receipt)?;
    scan_labels_reverse_node(left, unmatched_exits, output, receipt)?;
    Ok(())
}

fn scan_page_reverse(
    tokens: &[GreenToken],
    range: Range<usize>,
    route: &[RouteSide],
    page_id: u64,
    scan: &mut ReverseScan<'_>,
) -> Result<(), GreenError> {
    for local in range.rev() {
        scan.receipt.leaf_tokens_scanned += 1;
        match tokens[local] {
            GreenToken::Exit => *scan.unmatched_exits += 1,
            GreenToken::Enter { block, kind, .. } if *scan.unmatched_exits == 0 => {
                scan.output.push(GreenAncestor {
                    block,
                    kind,
                    enter: GreenTokenCursor {
                        root_identity: scan.root_identity,
                        route: Arc::from(route),
                        page_id,
                        local_index: u16::try_from(local)
                            .map_err(|_| GreenError::Overflow("leaf token index"))?,
                    },
                });
            }
            GreenToken::Enter { .. } => *scan.unmatched_exits -= 1,
            GreenToken::Property(_) | GreenToken::Coverage(_) => {}
        }
    }
    Ok(())
}

fn scan_node_reverse(
    node: &Arc<Node>,
    route: &mut Vec<RouteSide>,
    scan: &mut ReverseScan<'_>,
) -> Result<(), GreenError> {
    scan.receipt.tree_nodes_visited += 1;
    let (opens, closes) = node.summary.unmatched()?;
    if opens <= *scan.unmatched_exits {
        *scan.unmatched_exits = *scan.unmatched_exits - opens + closes;
        scan.receipt.summary_nodes_skipped += 1;
        return Ok(());
    }
    match &node.kind {
        NodeKind::Leaf {
            page_id, tokens, ..
        } => scan_page_reverse(tokens, 0..tokens.len(), route, *page_id, scan),
        NodeKind::Branch { left, right, .. } => {
            route.push(RouteSide::Right);
            scan_node_reverse(right, route, scan)?;
            route.pop();
            route.push(RouteSide::Left);
            scan_node_reverse(left, route, scan)?;
            route.pop();
            Ok(())
        }
    }
}

fn scan_page_forward_for_exit(
    tokens: &[GreenToken],
    range: Range<usize>,
    route: &[RouteSide],
    root_identity: usize,
    page_id: u64,
    depth: &mut i64,
    receipt: &mut GreenQueryReceipt,
) -> Result<Option<GreenTokenCursor>, GreenError> {
    for local in range {
        receipt.leaf_tokens_scanned += 1;
        match tokens[local] {
            GreenToken::Enter { .. } => *depth += 1,
            GreenToken::Exit => {
                *depth -= 1;
                if *depth == 0 {
                    return Ok(Some(GreenTokenCursor {
                        root_identity,
                        route: Arc::from(route),
                        page_id,
                        local_index: u16::try_from(local)
                            .map_err(|_| GreenError::Overflow("leaf token index"))?,
                    }));
                }
            }
            GreenToken::Property(_) | GreenToken::Coverage(_) => {}
        }
    }
    Ok(None)
}

fn find_exit_in_node(
    node: &Arc<Node>,
    route: &mut Vec<RouteSide>,
    root_identity: usize,
    depth: &mut i64,
    receipt: &mut GreenQueryReceipt,
) -> Result<Option<GreenTokenCursor>, GreenError> {
    receipt.tree_nodes_visited += 1;
    if *depth + node.summary.minimum_prefix > 0 {
        *depth += node.summary.balance;
        receipt.summary_nodes_skipped += 1;
        return Ok(None);
    }
    match &node.kind {
        NodeKind::Leaf {
            page_id, tokens, ..
        } => scan_page_forward_for_exit(
            tokens,
            0..tokens.len(),
            route,
            root_identity,
            *page_id,
            depth,
            receipt,
        ),
        NodeKind::Branch { left, right, .. } => {
            route.push(RouteSide::Left);
            let found = find_exit_in_node(left, route, root_identity, depth, receipt)?;
            route.pop();
            if found.is_some() {
                return Ok(found);
            }
            route.push(RouteSide::Right);
            let found = find_exit_in_node(right, route, root_identity, depth, receipt)?;
            route.pop();
            Ok(found)
        }
    }
}

fn summary_before_cursor(
    node: &Arc<Node>,
    route: &[RouteSide],
    local_index: usize,
    receipt: &mut GreenQueryReceipt,
) -> Result<GreenSummary, GreenError> {
    receipt.tree_nodes_visited += 1;
    if route.is_empty() {
        let NodeKind::Leaf { tokens, .. } = &node.kind else {
            return Err(GreenError::StaleCursor);
        };
        if local_index > tokens.len() {
            return Err(GreenError::StaleCursor);
        }
        receipt.leaf_tokens_scanned += local_index;
        return GreenSummary::from_tokens(&tokens[..local_index]);
    }
    let NodeKind::Branch { left, right, .. } = &node.kind else {
        return Err(GreenError::StaleCursor);
    };
    match route[0] {
        RouteSide::Left => summary_before_cursor(left, &route[1..], local_index, receipt),
        RouteSide::Right => {
            receipt.summary_nodes_skipped += 1;
            left.summary.followed_by(summary_before_cursor(
                right,
                &route[1..],
                local_index,
                receipt,
            )?)
        }
    }
}

fn replace_at_route(
    node: Arc<Node>,
    route: &[RouteSide],
    local_index: usize,
    replacement: GreenToken,
    replacement_page_id: u64,
    receipt: &mut GreenMutationReceipt,
) -> Result<Arc<Node>, GreenError> {
    receipt.nodes_visited += 1;
    if route.is_empty() {
        let NodeKind::Leaf { tokens, .. } = &node.kind else {
            return Err(GreenError::StaleCursor);
        };
        if local_index >= tokens.len() {
            return Err(GreenError::StaleCursor);
        }
        let mut changed = tokens.to_vec();
        changed[local_index] = replacement;
        return Node::leaf(replacement_page_id, Arc::from(changed), receipt);
    }
    let NodeKind::Branch { left, right, .. } = &node.kind else {
        return Err(GreenError::StaleCursor);
    };
    match route[0] {
        RouteSide::Left => Node::branch(
            replace_at_route(
                left.clone(),
                &route[1..],
                local_index,
                replacement,
                replacement_page_id,
                receipt,
            )?,
            right.clone(),
            receipt,
        ),
        RouteSide::Right => Node::branch(
            left.clone(),
            replace_at_route(
                right.clone(),
                &route[1..],
                local_index,
                replacement,
                replacement_page_id,
                receipt,
            )?,
            receipt,
        ),
    }
}

fn split_at_route(
    node: Arc<Node>,
    route: &[RouteSide],
    local_index: usize,
    next_page_id: &mut u64,
    receipt: &mut GreenMutationReceipt,
) -> Result<NodeSplit, GreenError> {
    receipt.nodes_visited += 1;
    if route.is_empty() {
        let NodeKind::Leaf { tokens, .. } = &node.kind else {
            return Err(GreenError::StaleCursor);
        };
        if local_index > tokens.len() {
            return Err(GreenError::StaleCursor);
        }
        if local_index == 0 {
            return Ok((None, Some(node)));
        }
        if local_index == tokens.len() {
            return Ok((Some(node), None));
        }
        let left = Node::leaf(*next_page_id, Arc::from(&tokens[..local_index]), receipt)?;
        *next_page_id += 1;
        let right = Node::leaf(*next_page_id, Arc::from(&tokens[local_index..]), receipt)?;
        *next_page_id += 1;
        return Ok((Some(left), Some(right)));
    }
    let NodeKind::Branch { left, right, .. } = &node.kind else {
        return Err(GreenError::StaleCursor);
    };
    match route[0] {
        RouteSide::Left => {
            let (before, after_left) = split_at_route(
                left.clone(),
                &route[1..],
                local_index,
                next_page_id,
                receipt,
            )?;
            Ok((
                before,
                join_optional(after_left, Some(right.clone()), receipt)?,
            ))
        }
        RouteSide::Right => {
            let (before_right, after) = split_at_route(
                right.clone(),
                &route[1..],
                local_index,
                next_page_id,
                receipt,
            )?;
            Ok((
                join_optional(Some(left.clone()), before_right, receipt)?,
                after,
            ))
        }
    }
}

fn push_streaming_root(
    mut root: Arc<Node>,
    bins: &mut Vec<Option<Arc<Node>>>,
    receipt: &mut GreenMutationReceipt,
) -> Result<(), GreenError> {
    let mut level = 0;
    loop {
        if level == bins.len() {
            bins.push(Some(root));
            break;
        }
        if let Some(prefix) = bins[level].take() {
            root = Node::branch(prefix, root, receipt)?;
            level += 1;
        } else {
            bins[level] = Some(root);
            break;
        }
    }
    let live = bins.iter().filter(|entry| entry.is_some()).count();
    receipt.maximum_streaming_roots = receipt.maximum_streaming_roots.max(live);
    receipt.maximum_streaming_bin_bytes = receipt
        .maximum_streaming_bin_bytes
        .max(bins.capacity() * std::mem::size_of::<Option<Arc<Node>>>());
    Ok(())
}

fn join_optional(
    left: Option<Arc<Node>>,
    right: Option<Arc<Node>>,
    receipt: &mut GreenMutationReceipt,
) -> Result<Option<Arc<Node>>, GreenError> {
    match (left, right) {
        (None, right) => Ok(right),
        (left, None) => Ok(left),
        (Some(left), Some(right)) => join(left, right, receipt).map(Some),
    }
}

fn join(
    left: Arc<Node>,
    right: Arc<Node>,
    receipt: &mut GreenMutationReceipt,
) -> Result<Arc<Node>, GreenError> {
    receipt.nodes_visited += 1;
    if left.height > right.height.saturating_add(1) {
        let NodeKind::Branch {
            left: left_left,
            right: left_right,
            ..
        } = &left.kind
        else {
            return Err(GreenError::Corrupt("unbalanced leaf height"));
        };
        let joined = join(left_right.clone(), right, receipt)?;
        return balance(left_left.clone(), joined, receipt);
    }
    if right.height > left.height.saturating_add(1) {
        let NodeKind::Branch {
            left: right_left,
            right: right_right,
            ..
        } = &right.kind
        else {
            return Err(GreenError::Corrupt("unbalanced leaf height"));
        };
        let joined = join(left, right_left.clone(), receipt)?;
        return balance(joined, right_right.clone(), receipt);
    }
    Node::branch(left, right, receipt)
}

fn balance(
    left: Arc<Node>,
    right: Arc<Node>,
    receipt: &mut GreenMutationReceipt,
) -> Result<Arc<Node>, GreenError> {
    if left.height > right.height.saturating_add(1) {
        let NodeKind::Branch {
            left: a, right: b, ..
        } = &left.kind
        else {
            return Err(GreenError::Corrupt("left-heavy leaf"));
        };
        if a.height >= b.height {
            return Node::branch(a.clone(), Node::branch(b.clone(), right, receipt)?, receipt);
        }
        let NodeKind::Branch {
            left: b1,
            right: b2,
            ..
        } = &b.kind
        else {
            return Err(GreenError::Corrupt("left double rotation leaf"));
        };
        return Node::branch(
            Node::branch(a.clone(), b1.clone(), receipt)?,
            Node::branch(b2.clone(), right, receipt)?,
            receipt,
        );
    }
    if right.height > left.height.saturating_add(1) {
        let NodeKind::Branch {
            left: b, right: c, ..
        } = &right.kind
        else {
            return Err(GreenError::Corrupt("right-heavy leaf"));
        };
        if c.height >= b.height {
            return Node::branch(Node::branch(left, b.clone(), receipt)?, c.clone(), receipt);
        }
        let NodeKind::Branch {
            left: b1,
            right: b2,
            ..
        } = &b.kind
        else {
            return Err(GreenError::Corrupt("right double rotation leaf"));
        };
        return Node::branch(
            Node::branch(left, b1.clone(), receipt)?,
            Node::branch(b2.clone(), c.clone(), receipt)?,
            receipt,
        );
    }
    Node::branch(left, right, receipt)
}

fn collect_shared_memory(
    mut stack: Vec<Arc<Node>>,
    seen: &mut HashSet<usize>,
    stats: &mut GreenMemoryStats,
) {
    while let Some(node) = stack.pop() {
        if !seen.insert(Arc::as_ptr(&node) as usize) {
            continue;
        }
        match &node.kind {
            NodeKind::Leaf {
                tokens, encoded, ..
            } => {
                stats.leaf_pages += 1;
                stats.tokens += tokens.len();
                stats.packed_token_bytes += encoded.len() - GREEN_LEAF_HEADER_BYTES;
                stats.retained_payload_bytes += encoded.len();
                for token in tokens.iter().copied() {
                    match token {
                        GreenToken::Enter { .. } => stats.semantic_blocks += 1,
                        GreenToken::Property(_) => stats.property_records += 1,
                        GreenToken::Coverage(_) => stats.coverage_atoms += 1,
                        GreenToken::Exit => {}
                    }
                }
            }
            NodeKind::Branch {
                left,
                right,
                encoded_summary,
            } => {
                stats.branch_nodes += 1;
                stats.retained_payload_bytes += encoded_summary.len();
                stack.push(left.clone());
                stack.push(right.clone());
            }
        }
    }
}

fn finalize_memory_stats(stats: &mut GreenMemoryStats) {
    stats.arena_slots = stats.leaf_pages + stats.branch_nodes;
    stats.arena_slot_bytes = stats.arena_slots * GREEN_ARENA_SLOT_BYTES;
    stats.accounted_retained_bytes = stats.retained_payload_bytes + stats.arena_slot_bytes;
    stats.prototype_typed_token_bytes = stats.tokens * std::mem::size_of::<GreenToken>();
    let arc_counters = 2 * std::mem::size_of::<usize>();
    stats.prototype_heap_lower_bound = stats.arena_slots
        * (std::mem::size_of::<Node>() + arc_counters)
        + stats.prototype_typed_token_bytes
        + (stats.retained_payload_bytes - stats.branch_nodes * GREEN_BRANCH_PAYLOAD_BYTES)
        + stats.leaf_pages * 2 * arc_counters;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GreenError {
    Invalid(&'static str),
    Corrupt(&'static str),
    Overflow(&'static str),
    SourceOutOfBounds,
    StaleCursor,
}

impl fmt::Display for GreenError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(message) => write!(formatter, "invalid serialized green: {message}"),
            Self::Corrupt(message) => write!(formatter, "corrupt serialized green: {message}"),
            Self::Overflow(field) => write!(formatter, "serialized green {field} overflow"),
            Self::SourceOutOfBounds => formatter.write_str("source coordinate is out of bounds"),
            Self::StaleCursor => formatter.write_str("serialized-green cursor is stale"),
        }
    }
}

impl std::error::Error for GreenError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn enter(id: u64) -> GreenToken {
        GreenToken::enter(
            BlockId(id),
            GreenKind::ITEM,
            ClosedChildSummary::from_bits((id & 7) as u8),
        )
    }

    fn brute_reduced(tokens: &[GreenToken]) -> (u64, Vec<OpenLabel>) {
        let mut unmatched_closes = 0_u64;
        let mut opens = Vec::new();
        for token in tokens {
            match *token {
                GreenToken::Enter { block, kind, .. } => opens.push(OpenLabel { block, kind }),
                GreenToken::Exit => {
                    if opens.pop().is_none() {
                        unmatched_closes += 1;
                    }
                }
                GreenToken::Property(_) | GreenToken::Coverage(_) => {}
            }
        }
        (unmatched_closes, opens)
    }

    #[test]
    fn fixed_open_witness_matches_every_short_fragment_and_split() {
        let property = GreenToken::Property(
            GreenProperty::new(PropertyTag::ITEM, &[2]).expect("test property"),
        );
        let alphabet = [enter(1), enter(2), GreenToken::Exit, property];
        let mut sequences = vec![Vec::new()];
        for length in 1..=7 {
            let previous = sequences
                .iter()
                .filter(|sequence| sequence.len() == length - 1)
                .cloned()
                .collect::<Vec<_>>();
            for prefix in previous {
                for token in alphabet {
                    let mut sequence = prefix.clone();
                    sequence.push(token);
                    sequences.push(sequence);
                }
            }
        }
        let mut cases = 0_usize;
        for sequence in sequences {
            let exact = GreenSummary::from_tokens(&sequence).expect("summary");
            let (closes, opens) = brute_reduced(&sequence);
            assert_eq!(exact.unmatched().unwrap(), (opens.len() as u64, closes));
            let retained = opens.len().min(OPEN_WITNESS_LABELS);
            assert_eq!(
                &exact.first_unmatched_opens[..retained],
                &opens[..retained],
                "{sequence:?}"
            );
            for split in 0..=sequence.len() {
                let left = GreenSummary::from_tokens(&sequence[..split]).unwrap();
                let right = GreenSummary::from_tokens(&sequence[split..]).unwrap();
                assert_eq!(left.followed_by(right).unwrap(), exact, "{sequence:?}");
                cases += 1;
            }
        }
        eprintln!("serialized_green_witness_exhaustive cases={cases}");
        assert!(cases > 100_000);
    }
}
