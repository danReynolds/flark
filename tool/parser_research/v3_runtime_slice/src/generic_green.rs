//! Generic packed hierarchical-green discriminator.
//!
//! This module is deliberately isolated from the production-candidate arena.
//! It tests the one structural question that the specialized `green_tree`
//! witness cannot answer: can a bounded packed page own many heterogeneous
//! out-of-line subtrees without proxy nodes or pattern-specific microtrees?
//!
//! The page codec stores explicit local parent/subtree topology. It is not an
//! Enter/Exit event stream. Source atoms and external child regions are
//! interleaved in source order, so a continuation-line container marker may be
//! owned by an ancestor while it occurs inside a paragraph's source hull.

use std::collections::{HashSet, VecDeque};
use std::fmt;
use std::ops::Range;

const STORAGE_CAP_BYTES: usize = 4 * 1024;
const MAX_VARIABLE_EDGES: usize = 128;
const EDGE_BYTES: usize = 8;
const PAGE_TAG: u8 = 0xa1;
const PAGE_VERSION: u8 = 1;
const PAGE_HEADER_BYTES: usize = 32;
const NODE_BYTES: usize = 16;
const PIECE_BYTES: usize = 32;
const NO_LOCAL_NODE: u16 = u16::MAX;
const NO_EDGE: u16 = u16::MAX;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GenericGreenMetric {
    pub bytes: u64,
    pub utf16: u64,
}

impl GenericGreenMetric {
    fn checked_add(self, other: Self) -> Result<Self, GenericGreenError> {
        Ok(Self {
            bytes: self
                .bytes
                .checked_add(other.bytes)
                .ok_or(GenericGreenError::Overflow("source bytes"))?,
            utf16: self
                .utf16
                .checked_add(other.utf16)
                .ok_or(GenericGreenError::Overflow("source UTF-16"))?,
        })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct VariablePageId {
    pub index: u32,
    pub generation: u32,
}

#[derive(Debug, PartialEq, Eq)]
pub struct VariablePageOwner {
    id: VariablePageId,
}

impl VariablePageOwner {
    #[must_use]
    pub const fn id(&self) -> VariablePageId {
        self.id
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GenericGreenError {
    StorageTooLarge(usize),
    TooManyEdges(usize),
    StalePage(VariablePageId),
    NoOwnedReference(VariablePageId),
    ReferenceCountOverflow(VariablePageId),
    NodeIndexOverflow,
    GenerationExhausted(VariablePageId),
    Invalid(&'static str),
    Corrupt(&'static str),
    Overflow(&'static str),
    NotFound,
}

impl fmt::Display for GenericGreenError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StorageTooLarge(bytes) => write!(
                formatter,
                "generic green page uses {bytes} bytes; cap is {STORAGE_CAP_BYTES}"
            ),
            Self::TooManyEdges(edges) => write!(
                formatter,
                "generic green page has {edges} edges; cap is {MAX_VARIABLE_EDGES}"
            ),
            Self::StalePage(id) => write!(formatter, "stale variable-edge page {id:?}"),
            Self::NoOwnedReference(id) => {
                write!(
                    formatter,
                    "variable-edge page {id:?} has no owned reference"
                )
            }
            Self::ReferenceCountOverflow(id) => {
                write!(
                    formatter,
                    "variable-edge page {id:?} reference count overflow"
                )
            }
            Self::NodeIndexOverflow => formatter.write_str("variable-edge page index overflow"),
            Self::GenerationExhausted(id) => {
                write!(formatter, "variable-edge page {id:?} generation exhausted")
            }
            Self::Invalid(message) => write!(formatter, "invalid generic green page: {message}"),
            Self::Corrupt(message) => write!(formatter, "corrupt generic green page: {message}"),
            Self::Overflow(field) => write!(formatter, "generic green {field} overflow"),
            Self::NotFound => formatter.write_str("generic green value not found"),
        }
    }
}

impl std::error::Error for GenericGreenError {}

#[derive(Debug)]
struct VariableNode {
    payload_len: u16,
    edge_count: u16,
    storage: Box<[u8]>,
}

#[derive(Debug)]
struct VariableSlot {
    generation: u32,
    references: u32,
    owned_references: u32,
    scheduled_releases: u32,
    node: Option<VariableNode>,
    next_free: Option<u32>,
    queued: bool,
    retiring: bool,
    retire_edge: u16,
}

impl Default for VariableSlot {
    fn default() -> Self {
        Self {
            generation: 1,
            references: 0,
            owned_references: 0,
            scheduled_releases: 0,
            node: None,
            next_free: None,
            queued: false,
            retiring: false,
            retire_edge: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct VariableArenaMetrics {
    pub slots: usize,
    pub slot_capacity: usize,
    pub slot_storage_bytes: usize,
    pub live_nodes: usize,
    /// Payload and edge tables share these single page allocations.
    pub live_storage_bytes: usize,
    pub live_payload_bytes: usize,
    pub live_edge_bytes: usize,
    pub live_edges: usize,
    pub heap_page_allocations: usize,
    pub pending_pages: usize,
    pub high_water_live_nodes: usize,
    pub high_water_storage_bytes: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct VariableReclaimReceipt {
    pub transitions: usize,
    pub child_edges_released: usize,
    pub pages_reclaimed: usize,
    pub storage_bytes_reclaimed: usize,
    pub pending_after: usize,
}

/// Isolated bounded-variable-edge arena.
///
/// Each node has one heap allocation containing both its codec payload and its
/// encoded edge table. An edge is retired in its own fuelled transition; page
/// fanout therefore affects total work but never creates one proxy allocation
/// per child or an unbounded reclaim kernel.
#[derive(Debug, Default)]
pub struct VariableEdgeArena {
    slots: Vec<VariableSlot>,
    free_head: Option<u32>,
    pending: VecDeque<VariablePageId>,
    live_nodes: usize,
    live_storage_bytes: usize,
    live_payload_bytes: usize,
    live_edges: usize,
    high_water_live_nodes: usize,
    high_water_storage_bytes: usize,
}

impl VariableEdgeArena {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn allocate(
        &mut self,
        payload: &[u8],
        edges: &[VariablePageId],
    ) -> Result<VariablePageOwner, GenericGreenError> {
        if edges.len() > MAX_VARIABLE_EDGES {
            return Err(GenericGreenError::TooManyEdges(edges.len()));
        }
        let edge_bytes = edges
            .len()
            .checked_mul(EDGE_BYTES)
            .ok_or(GenericGreenError::Overflow("edge storage"))?;
        let storage_bytes = payload
            .len()
            .checked_add(edge_bytes)
            .ok_or(GenericGreenError::Overflow("page storage"))?;
        if storage_bytes > STORAGE_CAP_BYTES {
            return Err(GenericGreenError::StorageTooLarge(storage_bytes));
        }
        let payload_len = u16::try_from(payload.len())
            .map_err(|_| GenericGreenError::Overflow("payload length"))?;
        let edge_count =
            u16::try_from(edges.len()).map_err(|_| GenericGreenError::Overflow("edge count"))?;
        for edge in edges {
            let slot = self.slot(*edge)?;
            slot.references
                .checked_add(1)
                .ok_or(GenericGreenError::ReferenceCountOverflow(*edge))?;
        }

        let mut storage = Vec::with_capacity(storage_bytes);
        storage.extend_from_slice(payload);
        for edge in edges {
            push_u32(&mut storage, edge.index);
            push_u32(&mut storage, edge.generation);
        }
        debug_assert_eq!(storage.len(), storage_bytes);

        let index = self.take_slot()?;
        let generation = self.slots[index as usize].generation;
        let id = VariablePageId { index, generation };
        for edge in edges {
            self.slot_mut(*edge)?.references += 1;
        }
        let slot = &mut self.slots[index as usize];
        debug_assert!(slot.node.is_none() && slot.references == 0);
        slot.references = 1;
        slot.owned_references = 1;
        slot.scheduled_releases = 0;
        slot.queued = false;
        slot.retiring = false;
        slot.retire_edge = 0;
        slot.node = Some(VariableNode {
            payload_len,
            edge_count,
            storage: storage.into_boxed_slice(),
        });
        self.live_nodes += 1;
        self.live_storage_bytes += storage_bytes;
        self.live_payload_bytes += payload.len();
        self.live_edges += edges.len();
        self.high_water_live_nodes = self.high_water_live_nodes.max(self.live_nodes);
        self.high_water_storage_bytes = self.high_water_storage_bytes.max(self.live_storage_bytes);
        Ok(VariablePageOwner { id })
    }

    pub fn retain(&mut self, id: VariablePageId) -> Result<VariablePageOwner, GenericGreenError> {
        let slot = self.slot(id)?;
        let references = slot
            .references
            .checked_add(1)
            .ok_or(GenericGreenError::ReferenceCountOverflow(id))?;
        let owned = slot
            .owned_references
            .checked_add(1)
            .ok_or(GenericGreenError::ReferenceCountOverflow(id))?;
        let slot = self.slot_mut(id)?;
        slot.references = references;
        slot.owned_references = owned;
        Ok(VariablePageOwner { id })
    }

    #[allow(clippy::needless_pass_by_value)] // Moving the owner is the release proof.
    pub fn release_later(&mut self, owner: VariablePageOwner) -> Result<(), GenericGreenError> {
        let id = owner.id;
        let slot = self.slot_mut(id)?;
        if slot.owned_references == 0 {
            return Err(GenericGreenError::NoOwnedReference(id));
        }
        slot.owned_references -= 1;
        slot.scheduled_releases = slot
            .scheduled_releases
            .checked_add(1)
            .ok_or(GenericGreenError::ReferenceCountOverflow(id))?;
        self.enqueue(id);
        Ok(())
    }

    pub fn poll_reclaim(
        &mut self,
        fuel: usize,
    ) -> Result<VariableReclaimReceipt, GenericGreenError> {
        let mut receipt = VariableReclaimReceipt::default();
        while receipt.transitions < fuel {
            let Some(id) = self.pending.pop_front() else {
                break;
            };
            {
                let slot = self.slot_mut(id)?;
                slot.queued = false;
            }
            let scheduled = self.slot(id)?.scheduled_releases;
            if scheduled != 0 {
                let becomes_retiring = {
                    let slot = self.slot_mut(id)?;
                    slot.scheduled_releases -= 1;
                    slot.references = slot
                        .references
                        .checked_sub(1)
                        .ok_or(GenericGreenError::Corrupt("reference underflow"))?;
                    if slot.references == 0 {
                        slot.retiring = true;
                        true
                    } else {
                        false
                    }
                };
                receipt.transitions += 1;
                if becomes_retiring && self.edge_count(id)? == 0 {
                    self.finalize_page(id, &mut receipt)?;
                } else if self.slot(id)?.scheduled_releases != 0 || self.slot(id)?.retiring {
                    self.enqueue(id);
                }
                continue;
            }

            if self.slot(id)?.retiring {
                let edge_index = usize::from(self.slot(id)?.retire_edge);
                let edge_count = self.edge_count(id)?;
                if edge_index >= edge_count {
                    self.finalize_page(id, &mut receipt)?;
                    continue;
                }
                let child = self.edge_at(id, edge_index)?;
                {
                    let child_slot = self.slot_mut(child)?;
                    child_slot.references = child_slot
                        .references
                        .checked_sub(1)
                        .ok_or(GenericGreenError::Corrupt("child reference underflow"))?;
                    if child_slot.references == 0 {
                        child_slot.retiring = true;
                    }
                }
                if self.slot(child)?.retiring {
                    self.enqueue(child);
                }
                {
                    let slot = self.slot_mut(id)?;
                    slot.retire_edge += 1;
                }
                receipt.transitions += 1;
                receipt.child_edges_released += 1;
                if usize::from(self.slot(id)?.retire_edge) == edge_count {
                    self.finalize_page(id, &mut receipt)?;
                } else {
                    self.enqueue(id);
                }
            }
        }
        receipt.pending_after = self.pending.len();
        Ok(receipt)
    }

    #[must_use]
    pub fn metrics(&self) -> VariableArenaMetrics {
        VariableArenaMetrics {
            slots: self.slots.len(),
            slot_capacity: self.slots.capacity(),
            slot_storage_bytes: self.slots.capacity() * std::mem::size_of::<VariableSlot>(),
            live_nodes: self.live_nodes,
            live_storage_bytes: self.live_storage_bytes,
            live_payload_bytes: self.live_payload_bytes,
            live_edge_bytes: self.live_edges * EDGE_BYTES,
            live_edges: self.live_edges,
            heap_page_allocations: self.live_nodes,
            pending_pages: self.pending.len(),
            high_water_live_nodes: self.high_water_live_nodes,
            high_water_storage_bytes: self.high_water_storage_bytes,
        }
    }

    fn payload(&self, id: VariablePageId) -> Result<&[u8], GenericGreenError> {
        let node = self.node(id)?;
        Ok(&node.storage[..usize::from(node.payload_len)])
    }

    fn edge_count(&self, id: VariablePageId) -> Result<usize, GenericGreenError> {
        Ok(usize::from(self.node(id)?.edge_count))
    }

    fn edge_at(
        &self,
        id: VariablePageId,
        edge_index: usize,
    ) -> Result<VariablePageId, GenericGreenError> {
        let node = self.node(id)?;
        if edge_index >= usize::from(node.edge_count) {
            return Err(GenericGreenError::Corrupt("edge index out of range"));
        }
        let start = usize::from(node.payload_len) + edge_index * EDGE_BYTES;
        Ok(VariablePageId {
            index: read_u32(&node.storage[start..start + 4]),
            generation: read_u32(&node.storage[start + 4..start + 8]),
        })
    }

    fn node(&self, id: VariablePageId) -> Result<&VariableNode, GenericGreenError> {
        Ok(self.slot(id)?.node.as_ref().expect("live slot has node"))
    }

    fn slot(&self, id: VariablePageId) -> Result<&VariableSlot, GenericGreenError> {
        let slot = self
            .slots
            .get(id.index as usize)
            .ok_or(GenericGreenError::StalePage(id))?;
        if slot.generation != id.generation || slot.node.is_none() {
            return Err(GenericGreenError::StalePage(id));
        }
        Ok(slot)
    }

    fn slot_mut(&mut self, id: VariablePageId) -> Result<&mut VariableSlot, GenericGreenError> {
        let slot = self
            .slots
            .get_mut(id.index as usize)
            .ok_or(GenericGreenError::StalePage(id))?;
        if slot.generation != id.generation || slot.node.is_none() {
            return Err(GenericGreenError::StalePage(id));
        }
        Ok(slot)
    }

    fn take_slot(&mut self) -> Result<u32, GenericGreenError> {
        if let Some(index) = self.free_head {
            self.free_head = self.slots[index as usize].next_free.take();
            return Ok(index);
        }
        let index =
            u32::try_from(self.slots.len()).map_err(|_| GenericGreenError::NodeIndexOverflow)?;
        self.slots.push(VariableSlot::default());
        Ok(index)
    }

    fn enqueue(&mut self, id: VariablePageId) {
        let slot = &mut self.slots[id.index as usize];
        if !slot.queued {
            slot.queued = true;
            self.pending.push_back(id);
        }
    }

    fn finalize_page(
        &mut self,
        id: VariablePageId,
        receipt: &mut VariableReclaimReceipt,
    ) -> Result<(), GenericGreenError> {
        let index = id.index as usize;
        let slot = self.slot(id)?;
        if slot.references != 0
            || slot.scheduled_releases != 0
            || usize::from(slot.retire_edge) != usize::from(slot.node.as_ref().unwrap().edge_count)
        {
            return Err(GenericGreenError::Corrupt("premature page retirement"));
        }
        let node = self.slots[index].node.take().expect("validated node");
        let storage_bytes = node.storage.len();
        let payload_bytes = usize::from(node.payload_len);
        let edges = usize::from(node.edge_count);
        self.live_nodes -= 1;
        self.live_storage_bytes -= storage_bytes;
        self.live_payload_bytes -= payload_bytes;
        self.live_edges -= edges;
        receipt.pages_reclaimed += 1;
        receipt.storage_bytes_reclaimed += storage_bytes;
        let slot = &mut self.slots[index];
        slot.owned_references = 0;
        slot.queued = false;
        slot.retiring = false;
        slot.retire_edge = 0;
        slot.generation = slot
            .generation
            .checked_add(1)
            .ok_or(GenericGreenError::GenerationExhausted(id))?;
        slot.next_free = self.free_head;
        self.free_head = Some(id.index);
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum GenericBlockKind {
    Document = 1,
    BlockQuote = 2,
    List = 3,
    Item = 4,
    Table = 5,
    TableRow = 6,
    TableCell = 7,
    Paragraph = 8,
    Heading = 9,
    FencedCode = 10,
    Html = 11,
    ThematicBreak = 12,
    IndentedCode = 13,
}

impl GenericBlockKind {
    fn decode(value: u8) -> Result<Self, GenericGreenError> {
        match value {
            1 => Ok(Self::Document),
            2 => Ok(Self::BlockQuote),
            3 => Ok(Self::List),
            4 => Ok(Self::Item),
            5 => Ok(Self::Table),
            6 => Ok(Self::TableRow),
            7 => Ok(Self::TableCell),
            8 => Ok(Self::Paragraph),
            9 => Ok(Self::Heading),
            10 => Ok(Self::FencedCode),
            11 => Ok(Self::Html),
            12 => Ok(Self::ThematicBreak),
            13 => Ok(Self::IndentedCode),
            _ => Err(GenericGreenError::Corrupt("unknown block kind")),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum GenericSourceKind {
    Terminal = 1,
    Gap = 2,
    ContainerMarker = 3,
}

impl GenericSourceKind {
    fn decode(value: u8) -> Result<Self, GenericGreenError> {
        match value {
            1 => Ok(Self::Terminal),
            2 => Ok(Self::Gap),
            3 => Ok(Self::ContainerMarker),
            _ => Err(GenericGreenError::Corrupt("unknown source atom kind")),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GenericNodeSpec {
    pub block: u64,
    pub kind: GenericBlockKind,
    pub parent: Option<u16>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GenericPieceSpec {
    Source {
        owner: u16,
        kind: GenericSourceKind,
        coverage: u64,
        metric: GenericGreenMetric,
    },
    External {
        owner: u16,
        child: VariablePageId,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GenericPageSpec {
    pub nodes: Vec<GenericNodeSpec>,
    pub pieces: Vec<GenericPieceSpec>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct GenericGreenRoot {
    owner: VariablePageOwner,
}

impl GenericGreenRoot {
    #[must_use]
    pub const fn id(&self) -> VariablePageId {
        self.owner.id()
    }

    pub fn release_later(self, arena: &mut VariableEdgeArena) -> Result<(), GenericGreenError> {
        arena.release_later(self.owner)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GenericPageReceipt {
    pub payload_bytes: usize,
    pub edge_bytes: usize,
    pub total_storage_bytes: usize,
    pub local_nodes: usize,
    pub source_atoms: usize,
    pub external_edges: usize,
}

#[allow(clippy::too_many_lines)] // The encoder's validation is kept beside its fixed layout.
pub fn build_generic_green_page(
    arena: &mut VariableEdgeArena,
    spec: &GenericPageSpec,
    receipt: &mut GenericPageReceipt,
) -> Result<GenericGreenRoot, GenericGreenError> {
    validate_nodes(&spec.nodes)?;
    if spec.pieces.is_empty() {
        return Err(GenericGreenError::Invalid("page has no source pieces"));
    }
    let node_count = u16::try_from(spec.nodes.len())
        .map_err(|_| GenericGreenError::Overflow("local node count"))?;
    let piece_count =
        u16::try_from(spec.pieces.len()).map_err(|_| GenericGreenError::Overflow("piece count"))?;
    let mut metric = GenericGreenMetric::default();
    let mut edges = Vec::new();
    let mut coverage = HashSet::new();
    let mut encoded_pieces = Vec::with_capacity(spec.pieces.len());
    for piece in &spec.pieces {
        match *piece {
            GenericPieceSpec::Source {
                owner,
                kind,
                coverage: coverage_id,
                metric: piece_metric,
            } => {
                validate_owner(owner, spec.nodes.len())?;
                if coverage_id == 0 || !coverage.insert(coverage_id) {
                    return Err(GenericGreenError::Invalid(
                        "coverage IDs must be nonzero and page-local unique",
                    ));
                }
                if piece_metric.bytes == 0
                    || piece_metric.utf16 == 0
                    || piece_metric.bytes < piece_metric.utf16
                {
                    return Err(GenericGreenError::Invalid("invalid source atom metric"));
                }
                metric = metric.checked_add(piece_metric)?;
                encoded_pieces.push(DecodedPiece::Source {
                    owner,
                    kind,
                    coverage: coverage_id,
                    metric: piece_metric,
                });
            }
            GenericPieceSpec::External { owner, child } => {
                validate_owner(owner, spec.nodes.len())?;
                if edges.contains(&child) {
                    return Err(GenericGreenError::Invalid(
                        "one semantic page cannot attach the same child twice",
                    ));
                }
                let child_metric = page_metric(arena, child)?;
                metric = metric.checked_add(child_metric)?;
                let edge = u16::try_from(edges.len())
                    .map_err(|_| GenericGreenError::Overflow("edge index"))?;
                edges.push(child);
                encoded_pieces.push(DecodedPiece::External {
                    owner,
                    edge,
                    metric: child_metric,
                });
            }
        }
    }
    let edge_count =
        u16::try_from(edges.len()).map_err(|_| GenericGreenError::Overflow("edge count"))?;
    let subtree_ends = subtree_ends(&spec.nodes)?;
    let payload_len = PAGE_HEADER_BYTES
        .checked_add(
            spec.nodes
                .len()
                .checked_mul(NODE_BYTES)
                .ok_or(GenericGreenError::Overflow("node storage"))?,
        )
        .and_then(|value| value.checked_add(spec.pieces.len().checked_mul(PIECE_BYTES)?))
        .ok_or(GenericGreenError::Overflow("page payload"))?;
    let mut payload = Vec::with_capacity(payload_len);
    payload.push(PAGE_TAG);
    payload.push(PAGE_VERSION);
    push_u16(&mut payload, node_count);
    push_u16(&mut payload, piece_count);
    push_u16(&mut payload, edge_count);
    push_u64(&mut payload, metric.bytes);
    push_u64(&mut payload, metric.utf16);
    payload.extend_from_slice(&[0; 8]);
    debug_assert_eq!(payload.len(), PAGE_HEADER_BYTES);
    for (index, node) in spec.nodes.iter().enumerate() {
        push_u64(&mut payload, node.block);
        push_u16(&mut payload, node.parent.unwrap_or(NO_LOCAL_NODE));
        push_u16(&mut payload, subtree_ends[index]);
        payload.push(node.kind as u8);
        payload.push(0);
        push_u16(&mut payload, 0);
    }
    for piece in &encoded_pieces {
        encode_piece(*piece, &mut payload);
    }
    debug_assert_eq!(payload.len(), payload_len);
    let owner = arena.allocate(&payload, &edges)?;
    *receipt = GenericPageReceipt {
        payload_bytes: payload.len(),
        edge_bytes: edges.len() * EDGE_BYTES,
        total_storage_bytes: payload.len() + edges.len() * EDGE_BYTES,
        local_nodes: spec.nodes.len(),
        source_atoms: encoded_pieces
            .iter()
            .filter(|piece| matches!(piece, DecodedPiece::Source { .. }))
            .count(),
        external_edges: edges.len(),
    };
    Ok(GenericGreenRoot { owner })
}

pub fn splice_generic_page_pieces(
    arena: &mut VariableEdgeArena,
    root: VariablePageId,
    range: Range<usize>,
    replacements: &[GenericPieceSpec],
    receipt: &mut GenericPageReceipt,
) -> Result<GenericGreenRoot, GenericGreenError> {
    let decoded = decode_page(arena, root)?;
    if range.start > range.end || range.end > decoded.pieces.len() {
        return Err(GenericGreenError::Invalid(
            "piece splice range out of bounds",
        ));
    }
    let mut pieces = decoded
        .pieces
        .iter()
        .map(|piece| piece.to_spec(arena, root))
        .collect::<Result<Vec<_>, _>>()?;
    pieces.splice(range, replacements.iter().copied());
    build_generic_green_page(
        arena,
        &GenericPageSpec {
            nodes: decoded
                .nodes
                .iter()
                .map(|node| GenericNodeSpec {
                    block: node.block,
                    kind: node.kind,
                    parent: node.parent,
                })
                .collect(),
            pieces,
        },
        receipt,
    )
}

pub fn generic_page_metric(
    arena: &VariableEdgeArena,
    root: VariablePageId,
) -> Result<GenericGreenMetric, GenericGreenError> {
    Ok(decode_page(arena, root)?.metric)
}

/// Returns the page-bounded external child capabilities in source order.
///
/// This is a discriminator/debug query, not a document-wide `BlockId` index.
pub fn generic_page_external_children(
    arena: &VariableEdgeArena,
    root: VariablePageId,
) -> Result<Vec<VariablePageId>, GenericGreenError> {
    decode_page(arena, root)?
        .pieces
        .into_iter()
        .filter_map(|piece| match piece {
            DecodedPiece::External { edge, .. } => Some(arena.edge_at(root, usize::from(edge))),
            DecodedPiece::Source { .. } => None,
        })
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GenericCoordinate {
    Bytes,
    Utf16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GenericAffinity {
    Upstream,
    Downstream,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GenericQueryReceipt {
    pub pages_visited: usize,
    pub pieces_examined: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GenericSourceHit {
    pub owner: u64,
    pub coverage: u64,
    pub kind: GenericSourceKind,
    pub enclosing: Vec<u64>,
    pub receipt: GenericQueryReceipt,
}

pub fn generic_source_lookup(
    arena: &VariableEdgeArena,
    root: VariablePageId,
    mut offset: u64,
    coordinate: GenericCoordinate,
    affinity: GenericAffinity,
) -> Result<Option<GenericSourceHit>, GenericGreenError> {
    let total = coordinate_value(page_metric(arena, root)?, coordinate);
    if offset > total {
        return Ok(None);
    }
    let mut page = root;
    let mut ancestors = Vec::new();
    let mut receipt = GenericQueryReceipt::default();
    loop {
        receipt.pages_visited += 1;
        let decoded = decode_page(arena, page)?;
        let (piece_index, local_offset, examined) =
            select_piece(&decoded.pieces, offset, coordinate, affinity)?;
        receipt.pieces_examined += examined;
        let piece = decoded.pieces[piece_index];
        match piece {
            DecodedPiece::Source {
                owner,
                kind,
                coverage,
                ..
            } => {
                let enclosing = local_path(&decoded.nodes, owner, &ancestors)?;
                return Ok(Some(GenericSourceHit {
                    owner: *enclosing.last().ok_or(GenericGreenError::Corrupt(
                        "source owner has empty enclosing path",
                    ))?,
                    coverage,
                    kind,
                    enclosing,
                    receipt,
                }));
            }
            DecodedPiece::External { owner, edge, .. } => {
                ancestors = local_path(&decoded.nodes, owner, &ancestors)?;
                page = arena.edge_at(page, usize::from(edge))?;
                offset = local_offset;
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GenericViewportAtom {
    pub owner: u64,
    pub coverage: u64,
    pub kind: GenericSourceKind,
    pub enclosing: Vec<u64>,
    pub byte_start: u64,
    pub byte_end: u64,
    pub utf16_start: u64,
    pub utf16_end: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GenericViewportReceipt {
    pub pages_visited: usize,
    pub pieces_examined: usize,
    pub maximum_stack: usize,
}

struct ViewportPageTask {
    page: VariablePageId,
    base: GenericGreenMetric,
    ancestors: Vec<u64>,
}

pub fn generic_viewport_atoms(
    arena: &VariableEdgeArena,
    root: VariablePageId,
    range: Range<u64>,
    coordinate: GenericCoordinate,
    receipt: &mut GenericViewportReceipt,
) -> Result<Vec<GenericViewportAtom>, GenericGreenError> {
    if range.start > range.end {
        return Err(GenericGreenError::Invalid("viewport range is reversed"));
    }
    let mut output = Vec::new();
    let mut stack = vec![ViewportPageTask {
        page: root,
        base: GenericGreenMetric::default(),
        ancestors: Vec::new(),
    }];
    while let Some(task) = stack.pop() {
        receipt.pages_visited += 1;
        let decoded = decode_page(arena, task.page)?;
        let mut starts = Vec::with_capacity(decoded.pieces.len());
        let mut cursor = task.base;
        for piece in &decoded.pieces {
            starts.push(cursor);
            cursor = cursor.checked_add(piece.metric())?;
        }
        for (piece, start) in decoded.pieces.iter().zip(starts).rev() {
            receipt.pieces_examined += 1;
            let end = start.checked_add(piece.metric())?;
            let selected_start = coordinate_value(start, coordinate);
            let selected_end = coordinate_value(end, coordinate);
            if selected_start >= range.end || selected_end <= range.start {
                continue;
            }
            match *piece {
                DecodedPiece::Source {
                    owner,
                    kind,
                    coverage,
                    ..
                } => {
                    let enclosing = local_path(&decoded.nodes, owner, &task.ancestors)?;
                    output.push(GenericViewportAtom {
                        owner: *enclosing.last().ok_or(GenericGreenError::Corrupt(
                            "viewport source owner has empty path",
                        ))?,
                        coverage,
                        kind,
                        enclosing,
                        byte_start: start.bytes,
                        byte_end: end.bytes,
                        utf16_start: start.utf16,
                        utf16_end: end.utf16,
                    });
                }
                DecodedPiece::External { owner, edge, .. } => {
                    stack.push(ViewportPageTask {
                        page: arena.edge_at(task.page, usize::from(edge))?,
                        base: start,
                        ancestors: local_path(&decoded.nodes, owner, &task.ancestors)?,
                    });
                    receipt.maximum_stack = receipt.maximum_stack.max(stack.len());
                }
            }
        }
    }
    output.sort_by_key(|atom| (atom.byte_start, atom.byte_end));
    Ok(output)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DecodedNode {
    block: u64,
    parent: Option<u16>,
    subtree_end: u16,
    kind: GenericBlockKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DecodedPiece {
    Source {
        owner: u16,
        kind: GenericSourceKind,
        coverage: u64,
        metric: GenericGreenMetric,
    },
    External {
        owner: u16,
        edge: u16,
        metric: GenericGreenMetric,
    },
}

impl DecodedPiece {
    const fn metric(self) -> GenericGreenMetric {
        match self {
            Self::Source { metric, .. } | Self::External { metric, .. } => metric,
        }
    }

    fn to_spec(
        self,
        arena: &VariableEdgeArena,
        page: VariablePageId,
    ) -> Result<GenericPieceSpec, GenericGreenError> {
        Ok(match self {
            Self::Source {
                owner,
                kind,
                coverage,
                metric,
            } => GenericPieceSpec::Source {
                owner,
                kind,
                coverage,
                metric,
            },
            Self::External { owner, edge, .. } => GenericPieceSpec::External {
                owner,
                child: arena.edge_at(page, usize::from(edge))?,
            },
        })
    }
}

#[derive(Debug)]
struct DecodedPage {
    nodes: Vec<DecodedNode>,
    pieces: Vec<DecodedPiece>,
    metric: GenericGreenMetric,
}

fn decode_page(
    arena: &VariableEdgeArena,
    page: VariablePageId,
) -> Result<DecodedPage, GenericGreenError> {
    let payload = arena.payload(page)?;
    let mut decoder = Decoder::new(payload);
    if decoder.u8()? != PAGE_TAG || decoder.u8()? != PAGE_VERSION {
        return Err(GenericGreenError::Corrupt("wrong page header"));
    }
    let node_count = usize::from(decoder.u16()?);
    let piece_count = usize::from(decoder.u16()?);
    let edge_count = usize::from(decoder.u16()?);
    let declared_metric = GenericGreenMetric {
        bytes: decoder.u64()?,
        utf16: decoder.u64()?,
    };
    if decoder.take(8)? != [0; 8] || edge_count != arena.edge_count(page)? {
        return Err(GenericGreenError::Corrupt("page header mismatch"));
    }
    let mut nodes = Vec::with_capacity(node_count);
    for _ in 0..node_count {
        let block = decoder.u64()?;
        let raw_parent = decoder.u16()?;
        let subtree_end = decoder.u16()?;
        let kind = GenericBlockKind::decode(decoder.u8()?)?;
        if decoder.u8()? != 0 || decoder.u16()? != 0 {
            return Err(GenericGreenError::Corrupt("node padding"));
        }
        nodes.push(DecodedNode {
            block,
            parent: (raw_parent != NO_LOCAL_NODE).then_some(raw_parent),
            subtree_end,
            kind,
        });
    }
    let mut pieces = Vec::with_capacity(piece_count);
    for _ in 0..piece_count {
        pieces.push(decode_piece(&mut decoder)?);
    }
    if !decoder.is_empty() {
        return Err(GenericGreenError::Corrupt("trailing page payload"));
    }
    validate_decoded_nodes(&nodes)?;
    let mut actual_metric = GenericGreenMetric::default();
    let mut seen_edges = HashSet::new();
    for piece in &pieces {
        let owner = match *piece {
            DecodedPiece::Source { owner, .. } | DecodedPiece::External { owner, .. } => owner,
        };
        validate_owner(owner, nodes.len())?;
        if let DecodedPiece::External { edge, metric, .. } = *piece {
            if usize::from(edge) >= edge_count || !seen_edges.insert(edge) {
                return Err(GenericGreenError::Corrupt("invalid external edge index"));
            }
            let child = arena.edge_at(page, usize::from(edge))?;
            if page_metric(arena, child)? != metric {
                return Err(GenericGreenError::Corrupt("external metric mismatch"));
            }
        }
        actual_metric = actual_metric.checked_add(piece.metric())?;
    }
    if pieces.is_empty() || seen_edges.len() != edge_count || actual_metric != declared_metric {
        return Err(GenericGreenError::Corrupt("page summary mismatch"));
    }
    Ok(DecodedPage {
        nodes,
        pieces,
        metric: actual_metric,
    })
}

fn page_metric(
    arena: &VariableEdgeArena,
    page: VariablePageId,
) -> Result<GenericGreenMetric, GenericGreenError> {
    let payload = arena.payload(page)?;
    if payload.len() < PAGE_HEADER_BYTES || payload[0] != PAGE_TAG || payload[1] != PAGE_VERSION {
        return Err(GenericGreenError::Corrupt("wrong page metric header"));
    }
    Ok(GenericGreenMetric {
        bytes: read_u64(&payload[8..16]),
        utf16: read_u64(&payload[16..24]),
    })
}

fn validate_nodes(nodes: &[GenericNodeSpec]) -> Result<(), GenericGreenError> {
    if nodes.is_empty() || nodes.iter().any(|node| node.block == 0) {
        return Err(GenericGreenError::Invalid(
            "page must contain nonzero nodes",
        ));
    }
    let mut blocks = HashSet::new();
    for (index, node) in nodes.iter().enumerate() {
        if !blocks.insert(node.block) {
            return Err(GenericGreenError::Invalid("duplicate local block ID"));
        }
        if node
            .parent
            .is_some_and(|parent| usize::from(parent) >= index)
        {
            return Err(GenericGreenError::Invalid(
                "local parent must precede its child",
            ));
        }
    }
    Ok(())
}

fn validate_decoded_nodes(nodes: &[DecodedNode]) -> Result<(), GenericGreenError> {
    let specs = nodes
        .iter()
        .map(|node| GenericNodeSpec {
            block: node.block,
            kind: node.kind,
            parent: node.parent,
        })
        .collect::<Vec<_>>();
    validate_nodes(&specs)?;
    let expected = subtree_ends(&specs)?;
    if nodes
        .iter()
        .zip(expected)
        .any(|(node, end)| node.subtree_end != end)
    {
        return Err(GenericGreenError::Corrupt(
            "local subtree boundary mismatch",
        ));
    }
    Ok(())
}

fn subtree_ends(nodes: &[GenericNodeSpec]) -> Result<Vec<u16>, GenericGreenError> {
    let mut output = Vec::with_capacity(nodes.len());
    for root in 0..nodes.len() {
        let mut end = root + 1;
        while end < nodes.len() && is_descendant(nodes, end, root)? {
            end += 1;
        }
        output
            .push(u16::try_from(end).map_err(|_| GenericGreenError::Overflow("subtree boundary"))?);
    }
    Ok(output)
}

fn is_descendant(
    nodes: &[GenericNodeSpec],
    mut candidate: usize,
    ancestor: usize,
) -> Result<bool, GenericGreenError> {
    while let Some(parent) = nodes[candidate].parent {
        candidate = usize::from(parent);
        if candidate == ancestor {
            return Ok(true);
        }
        if candidate >= nodes.len() {
            return Err(GenericGreenError::Invalid("parent out of range"));
        }
    }
    Ok(false)
}

fn validate_owner(owner: u16, nodes: usize) -> Result<(), GenericGreenError> {
    if usize::from(owner) >= nodes {
        return Err(GenericGreenError::Invalid("piece owner out of range"));
    }
    Ok(())
}

fn local_path(
    nodes: &[DecodedNode],
    owner: u16,
    ancestors: &[u64],
) -> Result<Vec<u64>, GenericGreenError> {
    validate_owner(owner, nodes.len())?;
    let mut reverse = Vec::new();
    let mut current = Some(owner);
    while let Some(index) = current {
        let node = nodes
            .get(usize::from(index))
            .ok_or(GenericGreenError::Corrupt("local path escaped page"))?;
        reverse.push(node.block);
        current = node.parent;
    }
    reverse.reverse();
    let mut path = Vec::with_capacity(ancestors.len() + reverse.len());
    path.extend_from_slice(ancestors);
    path.extend(reverse);
    Ok(path)
}

fn select_piece(
    pieces: &[DecodedPiece],
    offset: u64,
    coordinate: GenericCoordinate,
    affinity: GenericAffinity,
) -> Result<(usize, u64, usize), GenericGreenError> {
    let mut cursor = 0_u64;
    for (index, piece) in pieces.iter().enumerate() {
        let length = coordinate_value(piece.metric(), coordinate);
        let end = cursor
            .checked_add(length)
            .ok_or(GenericGreenError::Overflow("query coordinate"))?;
        if offset < end
            || (offset == end
                && (affinity == GenericAffinity::Upstream || index + 1 == pieces.len()))
        {
            return Ok((index, offset.saturating_sub(cursor).min(length), index + 1));
        }
        cursor = end;
    }
    Err(GenericGreenError::NotFound)
}

const fn coordinate_value(metric: GenericGreenMetric, coordinate: GenericCoordinate) -> u64 {
    match coordinate {
        GenericCoordinate::Bytes => metric.bytes,
        GenericCoordinate::Utf16 => metric.utf16,
    }
}

fn encode_piece(piece: DecodedPiece, output: &mut Vec<u8>) {
    match piece {
        DecodedPiece::Source {
            owner,
            kind,
            coverage,
            metric,
        } => {
            output.push(1);
            output.push(kind as u8);
            push_u16(output, owner);
            push_u16(output, NO_EDGE);
            push_u16(output, 0);
            push_u64(output, metric.bytes);
            push_u64(output, metric.utf16);
            push_u64(output, coverage);
        }
        DecodedPiece::External {
            owner,
            edge,
            metric,
        } => {
            output.push(2);
            output.push(0);
            push_u16(output, owner);
            push_u16(output, edge);
            push_u16(output, 0);
            push_u64(output, metric.bytes);
            push_u64(output, metric.utf16);
            push_u64(output, 0);
        }
    }
}

fn decode_piece(decoder: &mut Decoder<'_>) -> Result<DecodedPiece, GenericGreenError> {
    let tag = decoder.u8()?;
    let kind = decoder.u8()?;
    let owner = decoder.u16()?;
    let edge = decoder.u16()?;
    if decoder.u16()? != 0 {
        return Err(GenericGreenError::Corrupt("piece padding"));
    }
    let metric = GenericGreenMetric {
        bytes: decoder.u64()?,
        utf16: decoder.u64()?,
    };
    let coverage = decoder.u64()?;
    match tag {
        1 if edge == NO_EDGE && coverage != 0 => Ok(DecodedPiece::Source {
            owner,
            kind: GenericSourceKind::decode(kind)?,
            coverage,
            metric,
        }),
        2 if kind == 0 && edge != NO_EDGE && coverage == 0 => Ok(DecodedPiece::External {
            owner,
            edge,
            metric,
        }),
        _ => Err(GenericGreenError::Corrupt("invalid piece tag")),
    }
}

fn push_u16(output: &mut Vec<u8>, value: u16) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn push_u32(output: &mut Vec<u8>, value: u32) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn push_u64(output: &mut Vec<u8>, value: u64) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn read_u32(value: &[u8]) -> u32 {
    u32::from_le_bytes(value.try_into().expect("four-byte scalar"))
}

fn read_u64(value: &[u8]) -> u64 {
    u64::from_le_bytes(value.try_into().expect("eight-byte scalar"))
}

struct Decoder<'a> {
    remaining: &'a [u8],
}

impl<'a> Decoder<'a> {
    const fn new(remaining: &'a [u8]) -> Self {
        Self { remaining }
    }

    const fn is_empty(&self) -> bool {
        self.remaining.is_empty()
    }

    fn take(&mut self, count: usize) -> Result<&'a [u8], GenericGreenError> {
        let (value, remaining) = self
            .remaining
            .split_at_checked(count)
            .ok_or(GenericGreenError::Corrupt("truncated page scalar"))?;
        self.remaining = remaining;
        Ok(value)
    }

    fn u8(&mut self) -> Result<u8, GenericGreenError> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, GenericGreenError> {
        Ok(u16::from_le_bytes(
            self.take(2)?.try_into().expect("two-byte scalar"),
        ))
    }

    fn u64(&mut self) -> Result<u64, GenericGreenError> {
        Ok(read_u64(self.take(8)?))
    }
}
