//! Persistent measured sequence backed by the production page arena.
//!
//! Grammar-specific users define only leaf payloads and one associative
//! semantic summary. This module owns structural measures, AVL balancing,
//! typed root authority, bounded resumable construction, and logarithmic
//! routing so SourceFacts, Green, projection, and reference indexes cannot
//! grow subtly different persistent sequence machinery.

use std::fmt;
use std::marker::PhantomData;
use std::ops::Range;

use crate::identity::ArenaId;
use crate::storage::{
    ArenaBuildKey, ArenaBuildOwner, ArenaBuildSession, ArenaError, BeginSealFailure,
    CandidateBuild, CandidateSeal, CommittedArenaRoot, PageArena,
};
use crate::ARENA_PAGE_BYTES;

const MAX_SEQUENCE_BIN_SLOTS: usize = u64::BITS as usize;
// Exact minimum leaf counts for AVL heights 0 through 92 when a leaf has
// height one. Height 93 cannot fit in the u64 leaf-count domain. Keeping this
// table fixed makes admission O(1) in the machine domain rather than adding a
// Fibonacci loop to every node decode during an O(log n) descent.
const AVL_MIN_LEAVES_BY_HEIGHT: [u64; 93] = [
    0,
    1,
    2,
    3,
    5,
    8,
    13,
    21,
    34,
    55,
    89,
    144,
    233,
    377,
    610,
    987,
    1_597,
    2_584,
    4_181,
    6_765,
    10_946,
    17_711,
    28_657,
    46_368,
    75_025,
    121_393,
    196_418,
    317_811,
    514_229,
    832_040,
    1_346_269,
    2_178_309,
    3_524_578,
    5_702_887,
    9_227_465,
    14_930_352,
    24_157_817,
    39_088_169,
    63_245_986,
    102_334_155,
    165_580_141,
    267_914_296,
    433_494_437,
    701_408_733,
    1_134_903_170,
    1_836_311_903,
    2_971_215_073,
    4_807_526_976,
    7_778_742_049,
    12_586_269_025,
    20_365_011_074,
    32_951_280_099,
    53_316_291_173,
    86_267_571_272,
    139_583_862_445,
    225_851_433_717,
    365_435_296_162,
    591_286_729_879,
    956_722_026_041,
    1_548_008_755_920,
    2_504_730_781_961,
    4_052_739_537_881,
    6_557_470_319_842,
    10_610_209_857_723,
    17_167_680_177_565,
    27_777_890_035_288,
    44_945_570_212_853,
    72_723_460_248_141,
    117_669_030_460_994,
    190_392_490_709_135,
    308_061_521_170_129,
    498_454_011_879_264,
    806_515_533_049_393,
    1_304_969_544_928_657,
    2_111_485_077_978_050,
    3_416_454_622_906_707,
    5_527_939_700_884_757,
    8_944_394_323_791_464,
    14_472_334_024_676_221,
    23_416_728_348_467_685,
    37_889_062_373_143_906,
    61_305_790_721_611_591,
    99_194_853_094_755_497,
    160_500_643_816_367_088,
    259_695_496_911_122_585,
    420_196_140_727_489_673,
    679_891_637_638_612_258,
    1_100_087_778_366_101_931,
    1_779_979_416_004_714_189,
    2_880_067_194_370_816_120,
    4_660_046_610_375_530_309,
    7_540_113_804_746_346_429,
    12_200_160_415_121_876_738,
];
pub(crate) const MAX_SEQUENCE_AVL_HEIGHT: u16 = (AVL_MIN_LEAVES_BY_HEIGHT.len() - 1) as u16;
// One atomic splice performs two bounded splits and two bounded joins. Each
// split may join once per authenticated height while unwinding, so the
// conservative absolute budget is quadratic in the fixed u64-domain height.
// A work unit is one decoded node header, summary combination, visited
// structural node, or allocated branch. The maximum is 138,384 units at
// height 92; production SourceFacts admission is far below this machine-domain
// adversary, but the absolute bound keeps the accepted worker quantum finite.
pub(crate) const MAX_SEQUENCE_ATOMIC_SPLICE_WORK_UNITS: u64 = {
    let levels = MAX_SEQUENCE_AVL_HEIGHT as u64 + 1;
    16 * levels * levels
};
const MAX_SEQUENCE_BUILDER_OWNER_RESERVATION: usize =
    MAX_SEQUENCE_BIN_SLOTS + MAX_SEQUENCE_AVL_HEIGHT as usize + 8;
// One resumable poll executes at most one join task. The largest task is a
// double-rotation schedule: two height reads, one decomposition, two child
// height reads, and one pivot decomposition. Each shallow sequence-node read
// decodes at most three headers and combines at most one branch summary.
const MAX_SEQUENCE_POLL_NODE_HEADERS: u64 = 18;
const MAX_SEQUENCE_POLL_SUMMARY_COMBINATIONS: u64 = 6;
const MAX_SEQUENCE_POLL_PAYLOAD_BYTES: u64 =
    MAX_SEQUENCE_POLL_NODE_HEADERS * ARENA_PAGE_BYTES as u64;

/// Shape-independent semantic summary plus mechanism-owned tree measures.
///
/// Only `summary` participates in the caller's associative algebra. `leaves`
/// and `height` are derived and checked by this module, never by prefix folds.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct SequenceMeasure<Summary> {
    summary: Summary,
    leaves: u64,
    height: u16,
}

impl<Summary: Copy> SequenceMeasure<Summary> {
    pub(crate) const fn new(summary: Summary, leaves: u64, height: u16) -> Self {
        Self {
            summary,
            leaves,
            height,
        }
    }

    pub(crate) const fn summary(self) -> Summary {
        self.summary
    }

    pub(crate) const fn leaves(self) -> u64 {
        self.leaves
    }

    pub(crate) const fn height(self) -> u16 {
        self.height
    }
}

/// Spec-owned inspection charged while decoding one immutable payload.
/// `spec_items_hashed` is intentionally grammar-neutral; SourceFacts defines
/// one item as one checkpoint fed to its content hasher.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct SequenceSpecInspection {
    pub(crate) payload_bytes_inspected: u64,
    pub(crate) spec_items_hashed: u64,
}

impl SequenceSpecInspection {
    pub(crate) fn charge_payload_bytes(&mut self, bytes: usize) -> Option<()> {
        self.payload_bytes_inspected = self
            .payload_bytes_inspected
            .checked_add(u64::try_from(bytes).ok()?)?;
        Some(())
    }

    pub(crate) fn charge_hashed_items(&mut self, items: usize) -> Option<()> {
        self.spec_items_hashed = self
            .spec_items_hashed
            .checked_add(u64::try_from(items).ok()?)?;
        Some(())
    }
}

/// Complete immutable-tree inspection performed by a query or mutation.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct SequenceInspectionReceipt {
    pub(crate) node_headers_decoded: u64,
    pub(crate) summary_combinations: u64,
    pub(crate) spec: SequenceSpecInspection,
    maximum_node_headers_decoded: Option<u64>,
    node_header_limit_exhausted: bool,
}

impl SequenceInspectionReceipt {
    /// Starts an otherwise-empty inspection with one exact positive
    /// node-header budget. The default receipt remains unlimited.
    pub(crate) fn with_node_header_limit(maximum: u64) -> Option<Self> {
        if maximum == 0 {
            return None;
        }
        Some(Self {
            maximum_node_headers_decoded: Some(maximum),
            ..Self::default()
        })
    }

    /// Whether an attempted header read was refused by the configured limit.
    ///
    /// This is deliberately distinct from the spec error returned by the
    /// measured operation, so a semantic query wrapper can map exhaustion to
    /// its typed budget outcome rather than treating it as corrupt storage.
    pub(crate) const fn node_header_limit_exhausted(self) -> bool {
        self.node_header_limit_exhausted
    }
}

/// Payload and associative-summary semantics for one persistent sequence.
pub(crate) trait SequenceSpec {
    type Summary: Copy + Eq + fmt::Debug;
    type Error: From<ArenaError>;

    fn leaf_summary(
        payload: &[u8],
        inspection: &mut SequenceSpecInspection,
    ) -> Result<Option<Self::Summary>, Self::Error>;
    fn branch_measure(
        payload: &[u8],
        inspection: &mut SequenceSpecInspection,
    ) -> Result<Option<SequenceMeasure<Self::Summary>>, Self::Error>;
    /// Encodes into fixed stack scratch and returns the exact nonzero length.
    fn encode_branch(
        measure: SequenceMeasure<Self::Summary>,
        output: &mut [u8; ARENA_PAGE_BYTES],
    ) -> Result<usize, Self::Error>;
    /// Composes adjacent semantic summaries. This operation must be
    /// associative and must fail closed on overflow or invalid input.
    fn combine(left: Self::Summary, right: Self::Summary) -> Result<Self::Summary, Self::Error>;
    fn invalid(message: &'static str) -> Self::Error;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SequenceNodeKind<Summary> {
    Leaf,
    Branch {
        left: ArenaId,
        left_measure: SequenceMeasure<Summary>,
        right: ArenaId,
        right_measure: SequenceMeasure<Summary>,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct DecodedSequenceNode<Summary> {
    measure: SequenceMeasure<Summary>,
    kind: SequenceNodeKind<Summary>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SequenceNodeHeaderKind {
    Leaf,
    Branch { left: ArenaId, right: ArenaId },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct DecodedSequenceNodeHeader<Summary> {
    measure: SequenceMeasure<Summary>,
    kind: SequenceNodeHeaderKind,
}

/// One located leaf plus the exact semantic summary of everything before it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct LocatedSequenceLeaf<Summary> {
    pub(crate) id: ArenaId,
    pub(crate) ordinal: u64,
    pub(crate) summary: Summary,
    /// `None` is the generic empty summary; specs do not invent a semantic
    /// identity solely for routing.
    pub(crate) prefix: Option<Summary>,
}

/// Direction in which a monotone semantic partition is accumulated.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SequenceSummaryPartitionDirection {
    Forward,
    Reverse,
}

/// Boundary leaf selected by a monotone semantic partition.
///
/// `accumulated` is the exact summary visited before the selected leaf in the
/// requested direction: the document prefix for `Forward`, or suffix for
/// `Reverse`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct LocatedSequenceSummaryPartition<Summary> {
    pub(crate) id: ArenaId,
    pub(crate) ordinal: u64,
    pub(crate) summary: Summary,
    pub(crate) accumulated: Option<Summary>,
}

/// Whether one direct ordered-leaf visitor should continue after the current
/// leaf.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SequenceLeafVisitControl {
    Continue,
    Stop,
}

/// Terminal shape of one direct ordered-leaf visit.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct SequenceLeafVisitReceipt {
    leaves_visited: u64,
    visitor_stopped: bool,
}

impl SequenceLeafVisitReceipt {
    pub(crate) const fn leaves_visited(self) -> u64 {
        self.leaves_visited
    }

    pub(crate) const fn visitor_stopped(self) -> bool {
        self.visitor_stopped
    }
}

#[derive(Clone, Copy)]
struct PendingSequenceSubtree<Summary> {
    id: ArenaId,
    expected: SequenceMeasure<Summary>,
    ordinal: u64,
    prefix: Option<Summary>,
}

/// Decodes and locally verifies one sequence node.
///
/// Production refs can only originate from a typed committed root built by
/// this module, so descendants were verified before sealing. Each read still
/// checks the visited node, direct child headers, branch algebra, AVL shape,
/// and a leaf-count-derived traversal bound before using a measure for routing.
fn sequence_node<Spec: SequenceSpec>(
    arena: &PageArena,
    id: ArenaId,
    inspection: &mut SequenceInspectionReceipt,
) -> Result<DecodedSequenceNode<Spec::Summary>, Spec::Error> {
    let header = sequence_node_header::<Spec>(arena, id, inspection)?;
    match header.kind {
        SequenceNodeHeaderKind::Leaf => Ok(DecodedSequenceNode {
            measure: header.measure,
            kind: SequenceNodeKind::Leaf,
        }),
        SequenceNodeHeaderKind::Branch { left, right } => {
            let left_measure = sequence_node_header::<Spec>(arena, left, inspection)?.measure;
            let right_measure = sequence_node_header::<Spec>(arena, right, inspection)?.measure;
            let expected = combine_measures::<Spec>(left_measure, right_measure, inspection)?;
            if header.measure != expected {
                return Err(Spec::invalid(
                    "sequence branch measure does not match its children",
                ));
            }
            Ok(DecodedSequenceNode {
                measure: header.measure,
                kind: SequenceNodeKind::Branch {
                    left,
                    left_measure,
                    right,
                    right_measure,
                },
            })
        }
    }
}

/// Validates one immutable sequence node and its direct measure relationship.
///
/// Snapshot hosts call this for every postorder node as it is admitted. Since
/// children are admitted before their parent, validating every node once
/// proves the complete measured closure incrementally without flattening it or
/// turning final installation into an unbounded traversal.
pub(crate) fn validate_measured_sequence_node<Spec: SequenceSpec>(
    arena: &PageArena,
    id: ArenaId,
    inspection: &mut SequenceInspectionReceipt,
) -> Result<SequenceMeasure<Spec::Summary>, Spec::Error> {
    sequence_node::<Spec>(arena, id, inspection).map(|node| node.measure)
}

/// Locates one leaf from a raw root that has already passed incremental
/// measured-sequence admission.
///
/// Typed producer roots use [`MeasuredSequenceRef`]. Independent hosts own an
/// arena root rather than a producer capability, so bounded role queries need
/// the same checked descent without pretending to own a typed committed root.
pub(crate) fn locate_measured_sequence_leaf_with_prefix<Spec: SequenceSpec>(
    arena: &PageArena,
    root: ArenaId,
    leaf_index: u64,
    inspection: &mut SequenceInspectionReceipt,
) -> Result<Option<LocatedSequenceLeaf<Spec::Summary>>, Spec::Error> {
    let mut id = root;
    let root = sequence_node::<Spec>(arena, id, inspection)?;
    if leaf_index >= root.measure.leaves {
        return Ok(None);
    }

    let mut expected = root.measure;
    let mut index = leaf_index;
    let mut prefix = None;
    for _ in 0..usize::from(root.measure.height) {
        let node = sequence_node::<Spec>(arena, id, inspection)?;
        if node.measure != expected {
            return Err(Spec::invalid(
                "sequence child measure changed during descent",
            ));
        }
        match node.kind {
            SequenceNodeKind::Leaf => {
                if index != 0 {
                    return Err(Spec::invalid(
                        "sequence leaf routing retained a nonzero index",
                    ));
                }
                return Ok(Some(LocatedSequenceLeaf {
                    id,
                    ordinal: leaf_index,
                    summary: node.measure.summary,
                    prefix,
                }));
            }
            SequenceNodeKind::Branch {
                left,
                left_measure,
                right,
                right_measure,
            } => {
                if index < left_measure.leaves {
                    id = left;
                    expected = left_measure;
                } else {
                    index = index
                        .checked_sub(left_measure.leaves)
                        .ok_or_else(|| Spec::invalid("sequence index underflow"))?;
                    prefix = Some(match prefix {
                        Some(prefix) => {
                            inspection.summary_combinations =
                                inspection.summary_combinations.checked_add(1).ok_or_else(
                                    || Spec::invalid("sequence summary combination count overflow"),
                                )?;
                            Spec::combine(prefix, left_measure.summary)?
                        }
                        None => left_measure.summary,
                    });
                    id = right;
                    expected = right_measure;
                }
            }
        }
    }
    Err(Spec::invalid("sequence descent exceeded its AVL height"))
}

/// Folds one ordered leaf range from a validated immutable root.
///
/// Fully covered subtrees contribute their stored semantic summary directly;
/// only the two boundary paths are descended. Work is therefore bounded by
/// AVL height, not by the number of leaves in the requested prefix or suffix.
pub(crate) fn measured_sequence_range_summary<Spec: SequenceSpec>(
    arena: &PageArena,
    root: ArenaId,
    range: Range<u64>,
    inspection: &mut SequenceInspectionReceipt,
) -> Result<Option<Spec::Summary>, Spec::Error> {
    if range.start > range.end {
        return Err(Spec::invalid("sequence summary range is reversed"));
    }
    if range.is_empty() {
        return Ok(None);
    }
    let root_node = sequence_node::<Spec>(arena, root, inspection)?;
    if range.end > root_node.measure.leaves {
        return Err(Spec::invalid("sequence summary range exceeds its root"));
    }
    measured_sequence_range_summary_inner::<Spec>(
        arena,
        root,
        root_node.measure,
        0,
        &range,
        inspection,
    )
}

fn measured_sequence_range_summary_inner<Spec: SequenceSpec>(
    arena: &PageArena,
    id: ArenaId,
    expected: SequenceMeasure<Spec::Summary>,
    start: u64,
    range: &Range<u64>,
    inspection: &mut SequenceInspectionReceipt,
) -> Result<Option<Spec::Summary>, Spec::Error> {
    let end = start
        .checked_add(expected.leaves)
        .ok_or_else(|| Spec::invalid("sequence summary boundary overflow"))?;
    if range.end <= start || range.start >= end {
        return Ok(None);
    }
    if range.start <= start && end <= range.end {
        return Ok(Some(expected.summary));
    }

    let node = sequence_node::<Spec>(arena, id, inspection)?;
    if node.measure != expected {
        return Err(Spec::invalid(
            "sequence child measure changed during range summary",
        ));
    }
    match node.kind {
        SequenceNodeKind::Leaf => Err(Spec::invalid(
            "sequence partial range terminated inside a leaf",
        )),
        SequenceNodeKind::Branch {
            left,
            left_measure,
            right,
            right_measure,
        } => {
            let right_start = start
                .checked_add(left_measure.leaves)
                .ok_or_else(|| Spec::invalid("sequence summary boundary overflow"))?;
            let left = measured_sequence_range_summary_inner::<Spec>(
                arena,
                left,
                left_measure,
                start,
                range,
                inspection,
            )?;
            let right = measured_sequence_range_summary_inner::<Spec>(
                arena,
                right,
                right_measure,
                right_start,
                range,
                inspection,
            )?;
            match (left, right) {
                (Some(left), Some(right)) => {
                    inspection.summary_combinations = inspection
                        .summary_combinations
                        .checked_add(1)
                        .ok_or_else(|| {
                            Spec::invalid("sequence summary combination count overflow")
                        })?;
                    Ok(Some(Spec::combine(left, right)?))
                }
                (Some(summary), None) | (None, Some(summary)) => Ok(Some(summary)),
                (None, None) => Ok(None),
            }
        }
    }
}

fn sequence_node_header<Spec: SequenceSpec>(
    arena: &PageArena,
    id: ArenaId,
    inspection: &mut SequenceInspectionReceipt,
) -> Result<DecodedSequenceNodeHeader<Spec::Summary>, Spec::Error> {
    if inspection
        .maximum_node_headers_decoded
        .is_some_and(|maximum| inspection.node_headers_decoded >= maximum)
    {
        inspection.node_header_limit_exhausted = true;
        return Err(Spec::invalid(
            "sequence node-header inspection limit exhausted",
        ));
    }
    let payload = arena.payload(id)?;
    inspection.node_headers_decoded = inspection
        .node_headers_decoded
        .checked_add(1)
        .ok_or_else(|| Spec::invalid("sequence header inspection count overflow"))?;
    inspection
        .spec
        .charge_payload_bytes(payload.len())
        .ok_or_else(|| Spec::invalid("sequence payload inspection count overflow"))?;
    let leaf = Spec::leaf_summary(payload, &mut inspection.spec)?;
    let branch = Spec::branch_measure(payload, &mut inspection.spec)?;
    match (leaf, branch) {
        (Some(_), Some(_)) => Err(Spec::invalid("sequence node encoding is ambiguous")),
        (Some(summary), None) => {
            if arena.child_count(id)? != 0 {
                return Err(Spec::invalid("sequence leaf owns children"));
            }
            Ok(DecodedSequenceNodeHeader {
                measure: SequenceMeasure {
                    summary,
                    leaves: 1,
                    height: 1,
                },
                kind: SequenceNodeHeaderKind::Leaf,
            })
        }
        (None, Some(measure)) => {
            validate_measure::<Spec>(measure, false)?;
            if arena.child_count(id)? != 2 {
                return Err(Spec::invalid("sequence branch has the wrong child count"));
            }
            let left = arena.child_at(id, 0)?;
            let right = arena.child_at(id, 1)?;
            Ok(DecodedSequenceNodeHeader {
                measure,
                kind: SequenceNodeHeaderKind::Branch { left, right },
            })
        }
        (None, None) => Err(Spec::invalid("unknown sequence node encoding")),
    }
}

fn validate_measure<Spec: SequenceSpec>(
    measure: SequenceMeasure<Spec::Summary>,
    leaf: bool,
) -> Result<(), Spec::Error> {
    if leaf {
        if measure.leaves != 1 || measure.height != 1 {
            return Err(Spec::invalid("sequence leaf measure is invalid"));
        }
        return Ok(());
    }
    if measure.leaves < 2 || measure.height < 2 {
        return Err(Spec::invalid("sequence branch measure is empty or scalar"));
    }
    if measure.height > MAX_SEQUENCE_AVL_HEIGHT
        || measure.height > maximum_avl_height(measure.leaves)
    {
        return Err(Spec::invalid(
            "sequence branch height is impossible for its leaf count",
        ));
    }
    Ok(())
}

fn combine_measures<Spec: SequenceSpec>(
    left: SequenceMeasure<Spec::Summary>,
    right: SequenceMeasure<Spec::Summary>,
    inspection: &mut SequenceInspectionReceipt,
) -> Result<SequenceMeasure<Spec::Summary>, Spec::Error> {
    validate_measure::<Spec>(left, left.leaves == 1)?;
    validate_measure::<Spec>(right, right.leaves == 1)?;
    if left.height.abs_diff(right.height) > 1 {
        return Err(Spec::invalid("sequence branch is not AVL balanced"));
    }
    inspection.summary_combinations = inspection
        .summary_combinations
        .checked_add(1)
        .ok_or_else(|| Spec::invalid("sequence summary combination count overflow"))?;
    let measure = SequenceMeasure {
        summary: Spec::combine(left.summary, right.summary)?,
        leaves: left
            .leaves
            .checked_add(right.leaves)
            .ok_or_else(|| Spec::invalid("sequence leaf count overflow"))?,
        height: left
            .height
            .max(right.height)
            .checked_add(1)
            .ok_or_else(|| Spec::invalid("sequence height overflow"))?,
    };
    validate_measure::<Spec>(measure, false)?;
    Ok(measure)
}

/// Maximum valid AVL height for a tree with exactly `leaves` leaves when a
/// leaf has height one.
pub(crate) fn maximum_avl_height(leaves: u64) -> u16 {
    if leaves == 0 {
        return 0;
    }
    let insertion = AVL_MIN_LEAVES_BY_HEIGHT.partition_point(|&minimum| minimum <= leaves);
    u16::try_from(insertion - 1).expect("AVL threshold table fits u16")
}

/// Exact worst-case header reads for [`MeasuredSequenceRef::locate_leaf_containing_metric`].
///
/// A leaf root is decoded once before descent and once in the descent loop.
/// For a taller tree, the initial branch read authenticates its own header and
/// both direct child headers. The loop then does the same for every branch on
/// the root-to-leaf path and reads the terminal leaf once:
/// `3 + 3 * (height - 1) + 1`.
pub(crate) const fn maximum_metric_lookup_node_headers(height: u16) -> u64 {
    match height {
        0 => 0,
        1 => 2,
        _ => 3 * height as u64 + 1,
    }
}

/// Typed non-owning view derived from a live committed owner.
#[derive(Debug)]
pub(crate) struct MeasuredSequenceRef<'root, Spec> {
    root: Option<ArenaId>,
    marker: PhantomData<&'root CommittedMeasuredSequenceRoot<Spec>>,
}

impl<Spec> Clone for MeasuredSequenceRef<'_, Spec> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<Spec> Copy for MeasuredSequenceRef<'_, Spec> {}

impl<'root, Spec: SequenceSpec> MeasuredSequenceRef<'root, Spec> {
    /// Reopens a raw root owned by an independently validated arena closure.
    ///
    /// Only schema-owned host import code may call this after every postorder
    /// node has passed [`validate_measured_sequence_node`]. The returned view
    /// owns nothing and must not outlive that arena root.
    pub(crate) const fn from_imported_root(
        root: Option<ArenaId>,
    ) -> MeasuredSequenceRef<'static, Spec> {
        MeasuredSequenceRef {
            root,
            marker: PhantomData,
        }
    }

    #[cfg(test)]
    const fn from_raw_root(root: Option<ArenaId>) -> MeasuredSequenceRef<'static, Spec> {
        Self::from_imported_root(root)
    }

    pub(crate) const fn root_id(self) -> Option<ArenaId> {
        self.root
    }

    pub(crate) fn summary(
        self,
        arena: &PageArena,
        inspection: &mut SequenceInspectionReceipt,
    ) -> Result<Option<SequenceMeasure<Spec::Summary>>, Spec::Error> {
        self.root
            .map(|root| sequence_node::<Spec>(arena, root, inspection).map(|node| node.measure))
            .transpose()
    }

    pub(crate) fn range_summary(
        self,
        arena: &PageArena,
        range: Range<u64>,
        inspection: &mut SequenceInspectionReceipt,
    ) -> Result<Option<Spec::Summary>, Spec::Error> {
        match self.root {
            Some(root) => measured_sequence_range_summary::<Spec>(arena, root, range, inspection),
            None if range.is_empty() => Ok(None),
            None => Err(Spec::invalid(
                "nonempty range requested from empty sequence",
            )),
        }
    }

    pub(crate) fn locate_leaf_with_prefix(
        self,
        arena: &PageArena,
        leaf_index: u64,
        inspection: &mut SequenceInspectionReceipt,
    ) -> Result<Option<LocatedSequenceLeaf<Spec::Summary>>, Spec::Error> {
        let Some(mut id) = self.root else {
            return Ok(None);
        };
        let root = sequence_node::<Spec>(arena, id, inspection)?;
        if leaf_index >= root.measure.leaves {
            return Ok(None);
        }

        let mut expected = root.measure;
        let mut index = leaf_index;
        let mut prefix = None;
        for _ in 0..usize::from(root.measure.height) {
            let node = sequence_node::<Spec>(arena, id, inspection)?;
            if node.measure != expected {
                return Err(Spec::invalid(
                    "sequence child measure changed during descent",
                ));
            }
            match node.kind {
                SequenceNodeKind::Leaf => {
                    if index != 0 {
                        return Err(Spec::invalid(
                            "sequence leaf routing retained a nonzero index",
                        ));
                    }
                    return Ok(Some(LocatedSequenceLeaf {
                        id,
                        ordinal: leaf_index,
                        summary: node.measure.summary,
                        prefix,
                    }));
                }
                SequenceNodeKind::Branch {
                    left,
                    left_measure,
                    right,
                    right_measure,
                } => {
                    if index < left_measure.leaves {
                        id = left;
                        expected = left_measure;
                    } else {
                        index = index
                            .checked_sub(left_measure.leaves)
                            .ok_or_else(|| Spec::invalid("sequence index underflow"))?;
                        prefix = Some(match prefix {
                            Some(prefix) => {
                                inspection.summary_combinations = inspection
                                    .summary_combinations
                                    .checked_add(1)
                                    .ok_or_else(|| {
                                        Spec::invalid("sequence summary combination count overflow")
                                    })?;
                                Spec::combine(prefix, left_measure.summary)?
                            }
                            None => left_measure.summary,
                        });
                        id = right;
                        expected = right_measure;
                    }
                }
            }
        }
        Err(Spec::invalid("sequence descent exceeded its AVL height"))
    }

    /// Finds the first leaf, in the requested direction, whose inclusion
    /// makes a monotone predicate true over an ordered summary range.
    ///
    /// Fully covered subtrees that do not cross the partition contribute
    /// their authenticated stored summary without descending. Work is bounded
    /// by the two range boundaries plus the selected AVL path.
    pub(crate) fn locate_leaf_by_monotone_summary(
        self,
        arena: &PageArena,
        range: Range<u64>,
        direction: SequenceSummaryPartitionDirection,
        inspection: &mut SequenceInspectionReceipt,
        mut predicate: impl FnMut(Spec::Summary) -> Result<bool, Spec::Error>,
    ) -> Result<Option<LocatedSequenceSummaryPartition<Spec::Summary>>, Spec::Error> {
        if range.start > range.end {
            return Err(Spec::invalid("sequence partition range is reversed"));
        }
        if range.is_empty() {
            return Ok(None);
        }
        let Some(root_id) = self.root else {
            return Err(Spec::invalid(
                "nonempty partition requested from empty sequence",
            ));
        };
        let root = sequence_node::<Spec>(arena, root_id, inspection)?;
        if range.end > root.measure.leaves {
            return Err(Spec::invalid("sequence partition range exceeds its root"));
        }
        let mut accumulated = None;
        locate_leaf_by_monotone_summary_inner::<Spec>(
            arena,
            root_id,
            root.measure,
            0,
            &range,
            direction,
            &mut accumulated,
            inspection,
            &mut predicate,
        )
    }

    /// Locates the leaf containing one scalar position in an additive summary
    /// metric and returns its exact leaf ordinal and semantic prefix.
    ///
    /// The caller supplies the metric projection because sequence storage is
    /// grammar-neutral. Every visited branch verifies that the projection is
    /// additive before trusting it for routing.
    pub(crate) fn locate_leaf_containing_metric(
        self,
        arena: &PageArena,
        position: u64,
        metric: impl Fn(Spec::Summary) -> u64,
        inspection: &mut SequenceInspectionReceipt,
    ) -> Result<Option<LocatedSequenceLeaf<Spec::Summary>>, Spec::Error> {
        let Some(mut id) = self.root else {
            return Ok(None);
        };
        let root = sequence_node::<Spec>(arena, id, inspection)?;
        if position >= metric(root.measure.summary) {
            return Ok(None);
        }

        let mut expected = root.measure;
        let mut local_position = position;
        let mut ordinal = 0_u64;
        let mut prefix = None;
        for _ in 0..usize::from(root.measure.height) {
            let node = sequence_node::<Spec>(arena, id, inspection)?;
            if node.measure != expected {
                return Err(Spec::invalid(
                    "sequence child measure changed during metric descent",
                ));
            }
            match node.kind {
                SequenceNodeKind::Leaf => {
                    let leaf_metric = metric(node.measure.summary);
                    if leaf_metric == 0 || local_position >= leaf_metric {
                        return Err(Spec::invalid(
                            "sequence metric descent reached an invalid leaf",
                        ));
                    }
                    return Ok(Some(LocatedSequenceLeaf {
                        id,
                        ordinal,
                        summary: node.measure.summary,
                        prefix,
                    }));
                }
                SequenceNodeKind::Branch {
                    left,
                    left_measure,
                    right,
                    right_measure,
                } => {
                    let left_metric = metric(left_measure.summary);
                    let right_metric = metric(right_measure.summary);
                    if left_metric
                        .checked_add(right_metric)
                        .is_none_or(|sum| sum != metric(node.measure.summary))
                    {
                        return Err(Spec::invalid("sequence metric projection is not additive"));
                    }
                    if local_position < left_metric {
                        id = left;
                        expected = left_measure;
                    } else {
                        local_position = local_position
                            .checked_sub(left_metric)
                            .ok_or_else(|| Spec::invalid("sequence metric position underflow"))?;
                        ordinal = ordinal
                            .checked_add(left_measure.leaves)
                            .ok_or_else(|| Spec::invalid("sequence ordinal overflow"))?;
                        prefix = Some(match prefix {
                            Some(prefix) => {
                                inspection.summary_combinations = inspection
                                    .summary_combinations
                                    .checked_add(1)
                                    .ok_or_else(|| {
                                        Spec::invalid("sequence summary combination count overflow")
                                    })?;
                                Spec::combine(prefix, left_measure.summary)?
                            }
                            None => left_measure.summary,
                        });
                        id = right;
                        expected = right_measure;
                    }
                }
            }
        }
        Err(Spec::invalid(
            "sequence metric descent exceeded its AVL height",
        ))
    }

    /// Visits consecutive leaves beginning at the leaf containing one
    /// position in an additive semantic metric.
    ///
    /// The initial descent is logarithmic. Right siblings deferred by that
    /// descent are retained in a fixed stack whose absolute bound is the
    /// machine-domain AVL height, then consumed in source order. Every
    /// subsequently visited branch is decoded once, so work is
    /// `O(log N + visited leaves)` rather than one logarithmic lookup per
    /// leaf. The callback is synchronous and borrowed; no tree cursor or arena
    /// identity can escape the call.
    pub(crate) fn visit_leaves_from_metric(
        self,
        arena: &PageArena,
        position: u64,
        metric: impl Fn(Spec::Summary) -> u64,
        inspection: &mut SequenceInspectionReceipt,
        mut visitor: impl FnMut(
            LocatedSequenceLeaf<Spec::Summary>,
        ) -> Result<SequenceLeafVisitControl, Spec::Error>,
    ) -> Result<SequenceLeafVisitReceipt, Spec::Error> {
        let Some(root_id) = self.root else {
            if position == 0 {
                return Ok(SequenceLeafVisitReceipt::default());
            }
            return Err(Spec::invalid(
                "nonzero metric position requested from empty sequence",
            ));
        };
        let root = sequence_node::<Spec>(arena, root_id, inspection)?;
        let total = metric(root.measure.summary);
        if total == 0 {
            return Err(Spec::invalid(
                "sequence visitor metric is empty for a nonempty root",
            ));
        }
        if position > total {
            return Err(Spec::invalid(
                "sequence visitor metric position exceeds its root",
            ));
        }
        if position == total {
            return Ok(SequenceLeafVisitReceipt::default());
        }

        let mut pending: [Option<PendingSequenceSubtree<Spec::Summary>>;
            MAX_SEQUENCE_AVL_HEIGHT as usize] = [None; MAX_SEQUENCE_AVL_HEIGHT as usize];
        let mut pending_len = 0_usize;
        let mut id = root_id;
        let mut expected = root.measure;
        let mut ordinal = 0_u64;
        let mut prefix = None;
        let mut local_position = Some(position);
        let mut leaves_visited = 0_u64;

        loop {
            let node = sequence_node::<Spec>(arena, id, inspection)?;
            if node.measure != expected {
                return Err(Spec::invalid(
                    "sequence child measure changed during ordered visit",
                ));
            }
            match node.kind {
                SequenceNodeKind::Leaf => {
                    let leaf_metric = metric(node.measure.summary);
                    if leaf_metric == 0
                        || local_position.is_some_and(|position| position >= leaf_metric)
                    {
                        return Err(Spec::invalid(
                            "sequence ordered visit reached an invalid leaf",
                        ));
                    }
                    leaves_visited = leaves_visited
                        .checked_add(1)
                        .ok_or_else(|| Spec::invalid("sequence leaf visit count overflow"))?;
                    if visitor(LocatedSequenceLeaf {
                        id,
                        ordinal,
                        summary: node.measure.summary,
                        prefix,
                    })? == SequenceLeafVisitControl::Stop
                    {
                        return Ok(SequenceLeafVisitReceipt {
                            leaves_visited,
                            visitor_stopped: true,
                        });
                    }

                    let Some(next_len) = pending_len.checked_sub(1) else {
                        return Ok(SequenceLeafVisitReceipt {
                            leaves_visited,
                            visitor_stopped: false,
                        });
                    };
                    pending_len = next_len;
                    let next = pending[pending_len]
                        .take()
                        .ok_or_else(|| Spec::invalid("sequence visit stack lost a subtree"))?;
                    id = next.id;
                    expected = next.expected;
                    ordinal = next.ordinal;
                    prefix = next.prefix;
                    local_position = None;
                }
                SequenceNodeKind::Branch {
                    left,
                    left_measure,
                    right,
                    right_measure,
                } => {
                    let left_metric = metric(left_measure.summary);
                    let right_metric = metric(right_measure.summary);
                    if left_metric
                        .checked_add(right_metric)
                        .is_none_or(|sum| sum != metric(node.measure.summary))
                    {
                        return Err(Spec::invalid(
                            "sequence ordered-visit metric is not additive",
                        ));
                    }
                    let right_ordinal = ordinal
                        .checked_add(left_measure.leaves)
                        .ok_or_else(|| Spec::invalid("sequence ordinal overflow"))?;
                    let right_prefix = Some(match prefix {
                        Some(prefix) => {
                            inspection.summary_combinations =
                                inspection.summary_combinations.checked_add(1).ok_or_else(
                                    || Spec::invalid("sequence summary combination count overflow"),
                                )?;
                            Spec::combine(prefix, left_measure.summary)?
                        }
                        None => left_measure.summary,
                    });

                    match local_position {
                        Some(position) if position >= left_metric => {
                            local_position =
                                Some(position.checked_sub(left_metric).ok_or_else(|| {
                                    Spec::invalid("sequence metric position underflow")
                                })?);
                            id = right;
                            expected = right_measure;
                            ordinal = right_ordinal;
                            prefix = right_prefix;
                        }
                        _ => {
                            if pending_len >= pending.len() {
                                return Err(Spec::invalid(
                                    "sequence ordered visit exceeded its AVL stack",
                                ));
                            }
                            pending[pending_len] = Some(PendingSequenceSubtree {
                                id: right,
                                expected: right_measure,
                                ordinal: right_ordinal,
                                prefix: right_prefix,
                            });
                            pending_len += 1;
                            id = left;
                            expected = left_measure;
                        }
                    }
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn locate_leaf_by_monotone_summary_inner<Spec: SequenceSpec>(
    arena: &PageArena,
    id: ArenaId,
    expected: SequenceMeasure<Spec::Summary>,
    start: u64,
    range: &Range<u64>,
    direction: SequenceSummaryPartitionDirection,
    accumulated: &mut Option<Spec::Summary>,
    inspection: &mut SequenceInspectionReceipt,
    predicate: &mut impl FnMut(Spec::Summary) -> Result<bool, Spec::Error>,
) -> Result<Option<LocatedSequenceSummaryPartition<Spec::Summary>>, Spec::Error> {
    let end = start
        .checked_add(expected.leaves)
        .ok_or_else(|| Spec::invalid("sequence partition boundary overflow"))?;
    if range.end <= start || range.start >= end {
        return Ok(None);
    }
    let fully_covered = range.start <= start && end <= range.end;
    if fully_covered {
        let candidate = match (*accumulated, direction) {
            (None, _) => expected.summary,
            (Some(prefix), SequenceSummaryPartitionDirection::Forward) => {
                inspection.summary_combinations = inspection
                    .summary_combinations
                    .checked_add(1)
                    .ok_or_else(|| {
                        Spec::invalid("sequence partition combination count overflow")
                    })?;
                Spec::combine(prefix, expected.summary)?
            }
            (Some(suffix), SequenceSummaryPartitionDirection::Reverse) => {
                inspection.summary_combinations = inspection
                    .summary_combinations
                    .checked_add(1)
                    .ok_or_else(|| {
                        Spec::invalid("sequence partition combination count overflow")
                    })?;
                Spec::combine(expected.summary, suffix)?
            }
        };
        if !predicate(candidate)? {
            *accumulated = Some(candidate);
            return Ok(None);
        }
    }

    let node = sequence_node::<Spec>(arena, id, inspection)?;
    if node.measure != expected {
        return Err(Spec::invalid(
            "sequence child measure changed during summary partition",
        ));
    }
    match node.kind {
        SequenceNodeKind::Leaf => {
            if !fully_covered {
                return Err(Spec::invalid(
                    "sequence summary partition terminated inside a leaf",
                ));
            }
            Ok(Some(LocatedSequenceSummaryPartition {
                id,
                ordinal: start,
                summary: expected.summary,
                accumulated: *accumulated,
            }))
        }
        SequenceNodeKind::Branch {
            left,
            left_measure,
            right,
            right_measure,
        } => {
            let right_start = start
                .checked_add(left_measure.leaves)
                .ok_or_else(|| Spec::invalid("sequence partition boundary overflow"))?;
            let (first_id, first_measure, first_start, second_id, second_measure, second_start) =
                match direction {
                    SequenceSummaryPartitionDirection::Forward => {
                        (left, left_measure, start, right, right_measure, right_start)
                    }
                    SequenceSummaryPartitionDirection::Reverse => {
                        (right, right_measure, right_start, left, left_measure, start)
                    }
                };
            if let Some(found) = locate_leaf_by_monotone_summary_inner::<Spec>(
                arena,
                first_id,
                first_measure,
                first_start,
                range,
                direction,
                accumulated,
                inspection,
                predicate,
            )? {
                return Ok(Some(found));
            }
            locate_leaf_by_monotone_summary_inner::<Spec>(
                arena,
                second_id,
                second_measure,
                second_start,
                range,
                direction,
                accumulated,
                inspection,
                predicate,
            )
        }
    }
}

/// Typed root selected from a still-active arena build.
pub(crate) struct MeasuredSequenceBuildRoot<Spec> {
    owner: ArenaBuildOwner,
    marker: PhantomData<Spec>,
}

impl<Spec> MeasuredSequenceBuildRoot<Spec> {
    /// Erases only the sequence type after the spec has validated the retained
    /// root. Ownership remains journalled by the same active arena build.
    pub(crate) fn into_owner(self) -> ArenaBuildOwner {
        self.owner
    }

    /// Borrows this still-journalled root for authenticated read-only routing.
    ///
    /// The returned view cannot escape the matching arena session. This is the
    /// build-local counterpart of a committed sequence reference and is used
    /// by parser transactions which must inspect an unpublished prefix before
    /// the enclosing candidate can be sealed.
    pub(crate) fn as_ref<'session>(
        &'session self,
        session: &'session ArenaBuildSession<'_>,
    ) -> Result<MeasuredSequenceRef<'session, Spec>, ArenaError> {
        session.validate_owner(&self.owner)?;
        Ok(MeasuredSequenceRef {
            root: Some(self.owner.id()),
            marker: PhantomData,
        })
    }
}

/// Validates one caller-owned journal handle as a measured root for `Spec`.
///
/// The handle is consumed only to prevent an unvalidated alias from surviving
/// successful promotion. On failure its arena reference remains owned by the
/// caller's active build journal, so aborting that journal is the sole cleanup
/// path; no raw [`ArenaId`] is ever promoted into typed authority.
pub(crate) fn validate_measured_sequence_build_owner<Spec: SequenceSpec>(
    session: &ArenaBuildSession<'_>,
    owner: ArenaBuildOwner,
    receipt: &mut SequenceMutationReceipt,
) -> Result<MeasuredSequenceBuildRoot<Spec>, Spec::Error> {
    session.validate_owner(&owner)?;
    sequence_node::<Spec>(session.arena(), owner.id(), &mut receipt.inspection)?;
    Ok(MeasuredSequenceBuildRoot {
        owner,
        marker: PhantomData,
    })
}

/// Typed seal capability; only it can mint a committed measured root.
#[must_use = "an armed measured-sequence seal must be polled to completion or explicitly aborted"]
pub(crate) struct MeasuredSequenceSeal<Spec> {
    seal: Option<CandidateSeal>,
    marker: PhantomData<Spec>,
}

/// Sole owner of one committed measured sequence root.
#[must_use = "a committed measured-sequence root must be transferred or explicitly released"]
pub(crate) struct CommittedMeasuredSequenceRoot<Spec> {
    root: Option<CommittedArenaRoot>,
    marker: PhantomData<Spec>,
}

impl<Spec> fmt::Debug for CommittedMeasuredSequenceRoot<Spec> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CommittedMeasuredSequenceRoot")
            .field("id", &self.root.as_ref().map(CommittedArenaRoot::id))
            .finish()
    }
}

impl<Spec> Drop for CommittedMeasuredSequenceRoot<Spec> {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            assert!(
                self.root.is_none(),
                "committed measured roots require explicit transfer or fuelled release"
            );
        }
    }
}

impl<Spec> CommittedMeasuredSequenceRoot<Spec> {
    pub(crate) fn as_ref(&self) -> MeasuredSequenceRef<'_, Spec> {
        MeasuredSequenceRef {
            root: self.root.as_ref().map(CommittedArenaRoot::id),
            marker: PhantomData,
        }
    }

    #[cfg(test)]
    pub(crate) fn root_id_for_test(&self) -> Option<ArenaId> {
        self.root.as_ref().map(CommittedArenaRoot::id)
    }

    pub(crate) fn release(
        mut self,
        arena: &mut PageArena,
    ) -> Result<(), CommittedMeasuredSequenceRootReleaseFailure<Spec>> {
        let root = self
            .root
            .take()
            .expect("typed committed roots always own one arena root");
        match arena.release_committed_root(root) {
            Ok(()) => Ok(()),
            Err(failure) => Err(CommittedMeasuredSequenceRootReleaseFailure {
                error: failure.error,
                root: Self {
                    root: Some(failure.root),
                    marker: PhantomData,
                },
            }),
        }
    }
}

pub(crate) struct CommittedMeasuredSequenceRootReleaseFailure<Spec> {
    pub(crate) error: ArenaError,
    pub(crate) root: CommittedMeasuredSequenceRoot<Spec>,
}

pub(crate) struct BeginMeasuredSequenceSealFailure<Spec> {
    pub(crate) error: ArenaError,
    pub(crate) build: CandidateBuild,
    pub(crate) root: MeasuredSequenceBuildRoot<Spec>,
}

pub(crate) struct AbortMeasuredSequenceSealFailure<Spec> {
    pub(crate) error: ArenaError,
    pub(crate) seal: MeasuredSequenceSeal<Spec>,
}

pub(crate) struct MeasuredSequenceSealPoll<Spec> {
    pub(crate) transitions: usize,
    pub(crate) remaining_non_root_owners: usize,
    pub(crate) root: Option<CommittedMeasuredSequenceRoot<Spec>>,
}

pub(crate) fn begin_measured_sequence_seal<Spec>(
    arena: &mut PageArena,
    build: CandidateBuild,
    root: MeasuredSequenceBuildRoot<Spec>,
) -> Result<MeasuredSequenceSeal<Spec>, BeginMeasuredSequenceSealFailure<Spec>> {
    match arena.begin_seal(build, root.owner) {
        Ok(seal) => Ok(MeasuredSequenceSeal {
            seal: Some(seal),
            marker: PhantomData,
        }),
        Err(BeginSealFailure { error, build, root }) => Err(BeginMeasuredSequenceSealFailure {
            error,
            build,
            root: MeasuredSequenceBuildRoot {
                owner: root,
                marker: PhantomData,
            },
        }),
    }
}

impl<Spec> MeasuredSequenceSeal<Spec> {
    pub(crate) fn poll(
        &mut self,
        arena: &mut PageArena,
        fuel: usize,
    ) -> Result<MeasuredSequenceSealPoll<Spec>, ArenaError> {
        let seal = self.seal.as_mut().ok_or(ArenaError::StaleBuild)?;
        let poll = arena.poll_seal(seal, fuel)?;
        if poll.root.is_some() {
            self.seal.take();
        }
        Ok(MeasuredSequenceSealPoll {
            transitions: poll.transitions,
            remaining_non_root_owners: poll.remaining_non_root_owners,
            root: poll.root.map(|root| CommittedMeasuredSequenceRoot {
                root: Some(root),
                marker: PhantomData,
            }),
        })
    }

    pub(crate) fn abort(
        mut self,
        arena: &mut PageArena,
    ) -> Result<(), AbortMeasuredSequenceSealFailure<Spec>> {
        let validation = self
            .seal
            .as_ref()
            .ok_or(ArenaError::StaleBuild)
            .and_then(|seal| arena.validate_seal(seal));
        if let Err(error) = validation {
            return Err(AbortMeasuredSequenceSealFailure { error, seal: self });
        }
        let seal = self
            .seal
            .take()
            .expect("validated measured seal remains armed");
        if let Err(error) = arena.abort_seal(seal) {
            panic!("validated measured seal abort failed: {error}");
        }
        Ok(())
    }
}

impl<Spec> Drop for MeasuredSequenceSeal<Spec> {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            assert!(
                self.seal.is_none(),
                "measured sequence seals require completion or explicit fuelled abort"
            );
        }
    }
}

/// Observable work and scratch high-water marks for one sequence build.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct SequenceMutationReceipt {
    pub(crate) inspection: SequenceInspectionReceipt,
    pub(crate) leaves_adopted: usize,
    pub(crate) branches_allocated: usize,
    pub(crate) branch_payload_bytes: usize,
    pub(crate) nodes_visited: usize,
    pub(crate) leaves_reused: usize,
    pub(crate) committed_leaves_retained: usize,
    pub(crate) leaves_deleted: usize,
    pub(crate) maximum_atomic_height: u16,
    pub(crate) maximum_live_bins: usize,
    pub(crate) maximum_join_tasks: usize,
    pub(crate) maximum_join_values: usize,
    pub(crate) reserved_owner_slots: usize,
    pub(crate) reserved_scratch_bytes: usize,
}

/// Retains a committed measured root directly into the caller's active build.
///
/// This is the narrow capability bridge used when a separately certified
/// replacement subtree is spliced into another persistent sequence.
pub(crate) fn retain_committed_measured_sequence_root<Spec: SequenceSpec>(
    session: &mut ArenaBuildSession<'_>,
    root: &CommittedMeasuredSequenceRoot<Spec>,
    receipt: &mut SequenceMutationReceipt,
) -> Result<MeasuredSequenceBuildRoot<Spec>, Spec::Error> {
    retain_committed_measured_sequence_root_with_measure(session, root, receipt)
        .map(|(root, _)| root)
}

type RetainedMeasuredSequenceRootWithMeasure<Spec> = (
    MeasuredSequenceBuildRoot<Spec>,
    SequenceMeasure<<Spec as SequenceSpec>::Summary>,
);

/// Retains a committed measured root and returns the exact locally validated
/// root measure from the same constant-work inspection.
pub(crate) fn retain_committed_measured_sequence_root_with_measure<Spec: SequenceSpec>(
    session: &mut ArenaBuildSession<'_>,
    root: &CommittedMeasuredSequenceRoot<Spec>,
    receipt: &mut SequenceMutationReceipt,
) -> Result<RetainedMeasuredSequenceRootWithMeasure<Spec>, Spec::Error> {
    let id = root
        .root
        .as_ref()
        .map(CommittedArenaRoot::id)
        .ok_or_else(|| Spec::invalid("committed sequence root was transferred"))?;
    let measure = sequence_node::<Spec>(session.arena(), id, &mut receipt.inspection)?.measure;
    let owner_reservation = session.remaining_owner_capacity()?.min(1);
    session.reserve_owner_capacity(owner_reservation)?;
    receipt.reserved_owner_slots = receipt.reserved_owner_slots.max(owner_reservation);
    let owner = session.retain(id)?;
    receipt.committed_leaves_retained = receipt
        .committed_leaves_retained
        .checked_add(
            usize::try_from(measure.leaves)
                .map_err(|_| Spec::invalid("retained leaf count exceeds usize"))?,
        )
        .ok_or_else(|| Spec::invalid("retained leaf count overflow"))?;
    Ok((
        MeasuredSequenceBuildRoot {
            owner,
            marker: PhantomData,
        },
        measure,
    ))
}

/// Retains one committed base and delegates to the owned-root splice.
///
/// Recursion is hard-bounded by the authenticated AVL height (at most 92 in
/// the u64 leaf domain, and far smaller under SourceFacts admission). Any
/// failure after the retain is terminal for the caller's build journal;
/// aborting that journal releases every intermediate owner without bespoke
/// rollback.
pub(crate) fn splice_measured_sequence_atomic<Spec: SequenceSpec>(
    session: &mut ArenaBuildSession<'_>,
    base: &CommittedMeasuredSequenceRoot<Spec>,
    range: Range<u64>,
    replacement: Option<MeasuredSequenceBuildRoot<Spec>>,
    receipt: &mut SequenceMutationReceipt,
) -> Result<Option<MeasuredSequenceBuildRoot<Spec>>, Spec::Error> {
    let base = retain_committed_measured_sequence_root(session, base, receipt)?;
    splice_measured_sequence_build_root_atomic(session, base, range, replacement, receipt)
}

/// Path-copies one bounded AVL splice from roots already owned by one build.
///
/// Both typed roots are consumed because split/join transfers their journalled
/// owners into the returned root. Every validation or mutation failure leaves
/// all surviving ownership in the caller's build journal; the caller must
/// abort that journal rather than attempting bespoke rollback.
pub(crate) fn splice_measured_sequence_build_root_atomic<Spec: SequenceSpec>(
    session: &mut ArenaBuildSession<'_>,
    base: MeasuredSequenceBuildRoot<Spec>,
    range: Range<u64>,
    replacement: Option<MeasuredSequenceBuildRoot<Spec>>,
    receipt: &mut SequenceMutationReceipt,
) -> Result<Option<MeasuredSequenceBuildRoot<Spec>>, Spec::Error> {
    let base_owner = base.owner;
    session.validate_owner(&base_owner)?;
    let base_measure =
        sequence_node::<Spec>(session.arena(), base_owner.id(), &mut receipt.inspection)?.measure;
    if range.start > range.end || range.end > base_measure.leaves {
        return Err(Spec::invalid("sequence splice range is invalid"));
    }
    let replacement_owner = replacement.map(|root| root.owner);
    let replacement_measure = replacement_owner
        .as_ref()
        .map(|owner| {
            session.validate_owner(owner)?;
            sequence_node::<Spec>(session.arena(), owner.id(), &mut receipt.inspection)
                .map(|node| node.measure)
        })
        .transpose()?;
    let replacement_leaves = replacement_measure
        .map(|measure| measure.leaves)
        .unwrap_or(0);
    let deleted_leaves = range.end - range.start;
    let target_leaves = base_measure
        .leaves
        .checked_sub(deleted_leaves)
        .and_then(|leaves| leaves.checked_add(replacement_leaves))
        .ok_or_else(|| Spec::invalid("sequence splice leaf count overflow"))?;
    receipt.maximum_atomic_height = receipt.maximum_atomic_height.max(base_measure.height);

    if deleted_leaves == 0 && replacement_owner.is_none() {
        receipt.leaves_reused = receipt
            .leaves_reused
            .checked_add(
                usize::try_from(base_measure.leaves)
                    .map_err(|_| Spec::invalid("reused leaf count exceeds usize"))?,
            )
            .ok_or_else(|| Spec::invalid("reused leaf count overflow"))?;
        return Ok(Some(MeasuredSequenceBuildRoot {
            owner: base_owner,
            marker: PhantomData,
        }));
    }

    // A split/join step replaces one owner by at most two before retiring the
    // input. Eight owner slots per authenticated level plus fixed endpoints is
    // deliberately conservative for the two splits and two joins. Reserving
    // once before the first retain prevents allocator work during mutation.
    let splice_height = base_measure.height.max(
        replacement_measure
            .map(|measure| measure.height)
            .unwrap_or(0),
    );
    let owner_reservation = usize::from(splice_height)
        .checked_mul(8)
        .and_then(|slots| slots.checked_add(16))
        .ok_or_else(|| Spec::invalid("sequence splice owner bound overflow"))?;
    let owner_reservation = owner_reservation.min(session.remaining_owner_capacity()?);
    session.reserve_owner_capacity(owner_reservation)?;
    receipt.reserved_owner_slots = receipt.reserved_owner_slots.max(owner_reservation);

    let (prefix, tail) = split_owned_atomic::<Spec>(session, base_owner, range.start, receipt)?;
    let (deleted, suffix) = match tail {
        Some(tail) => split_owned_atomic::<Spec>(session, tail, range.end - range.start, receipt)?,
        None if range.start == base_measure.leaves && range.is_empty() => (None, None),
        None => return Err(Spec::invalid("sequence splice lost its suffix")),
    };
    if let Some(deleted) = deleted {
        session.release(deleted)?;
    }
    let with_replacement =
        concat_owned_atomic::<Spec>(session, prefix, replacement_owner, receipt)?;
    let root = concat_owned_atomic::<Spec>(session, with_replacement, suffix, receipt)?;
    let observed_target = root
        .as_ref()
        .map(|owner| {
            sequence_node::<Spec>(session.arena(), owner.id(), &mut receipt.inspection)
                .map(|node| node.measure.leaves)
        })
        .transpose()?
        .unwrap_or(0);
    if observed_target != target_leaves {
        return Err(Spec::invalid(
            "sequence splice produced the wrong leaf count",
        ));
    }
    receipt.leaves_deleted = receipt
        .leaves_deleted
        .checked_add(
            usize::try_from(deleted_leaves)
                .map_err(|_| Spec::invalid("deleted leaf count exceeds usize"))?,
        )
        .ok_or_else(|| Spec::invalid("deleted leaf count overflow"))?;
    receipt.leaves_reused = receipt
        .leaves_reused
        .checked_add(
            usize::try_from(base_measure.leaves - deleted_leaves)
                .map_err(|_| Spec::invalid("reused leaf count exceeds usize"))?,
        )
        .ok_or_else(|| Spec::invalid("reused leaf count overflow"))?;
    Ok(root.map(|owner| MeasuredSequenceBuildRoot {
        owner,
        marker: PhantomData,
    }))
}

/// Joins two source-ordered roots already owned by the same active build.
///
/// This is intentionally an ownership operation, not an ID-based append. It
/// is the small primitive needed to force an unpublished prefix into one
/// readable root and later resume streaming into a fresh suffix builder.
pub(crate) fn concat_measured_sequence_build_roots_atomic<Spec: SequenceSpec>(
    session: &mut ArenaBuildSession<'_>,
    left: Option<MeasuredSequenceBuildRoot<Spec>>,
    right: Option<MeasuredSequenceBuildRoot<Spec>>,
    receipt: &mut SequenceMutationReceipt,
) -> Result<Option<MeasuredSequenceBuildRoot<Spec>>, Spec::Error> {
    let left = left.map(|root| root.owner);
    let right = right.map(|root| root.owner);
    concat_owned_atomic::<Spec>(session, left, right, receipt).map(|root| {
        root.map(|owner| MeasuredSequenceBuildRoot {
            owner,
            marker: PhantomData,
        })
    })
}

fn split_owned_atomic<Spec: SequenceSpec>(
    session: &mut ArenaBuildSession<'_>,
    node: ArenaBuildOwner,
    leaf_index: u64,
    receipt: &mut SequenceMutationReceipt,
) -> Result<(Option<ArenaBuildOwner>, Option<ArenaBuildOwner>), Spec::Error> {
    receipt.nodes_visited = receipt
        .nodes_visited
        .checked_add(1)
        .ok_or_else(|| Spec::invalid("sequence visit count overflow"))?;
    session.validate_owner(&node)?;
    let decoded = sequence_node::<Spec>(session.arena(), node.id(), &mut receipt.inspection)?;
    let leaves = decoded.measure.leaves;
    if leaf_index == 0 {
        return Ok((None, Some(node)));
    }
    if leaf_index == leaves {
        return Ok((Some(node), None));
    }
    if leaf_index > leaves {
        return Err(Spec::invalid("sequence split is out of range"));
    }
    let SequenceNodeKind::Branch {
        left,
        left_measure,
        right,
        ..
    } = decoded.kind
    else {
        return Err(Spec::invalid("sequence split reached a scalar leaf"));
    };
    let left = session.retain(left)?;
    let right = session.retain(right)?;
    session.release(node)?;
    match leaf_index.cmp(&left_measure.leaves) {
        std::cmp::Ordering::Less => {
            let (prefix, middle) = split_owned_atomic::<Spec>(session, left, leaf_index, receipt)?;
            let suffix = concat_owned_atomic::<Spec>(session, middle, Some(right), receipt)?;
            Ok((prefix, suffix))
        }
        std::cmp::Ordering::Equal => Ok((Some(left), Some(right))),
        std::cmp::Ordering::Greater => {
            let (middle, suffix) = split_owned_atomic::<Spec>(
                session,
                right,
                leaf_index - left_measure.leaves,
                receipt,
            )?;
            let prefix = concat_owned_atomic::<Spec>(session, Some(left), middle, receipt)?;
            Ok((prefix, suffix))
        }
    }
}

fn concat_owned_atomic<Spec: SequenceSpec>(
    session: &mut ArenaBuildSession<'_>,
    left: Option<ArenaBuildOwner>,
    right: Option<ArenaBuildOwner>,
    receipt: &mut SequenceMutationReceipt,
) -> Result<Option<ArenaBuildOwner>, Spec::Error> {
    match (left, right) {
        (None, value) | (value, None) => Ok(value),
        (Some(left), Some(right)) => {
            join_owned_atomic::<Spec>(session, left, right, receipt).map(Some)
        }
    }
}

fn join_owned_atomic<Spec: SequenceSpec>(
    session: &mut ArenaBuildSession<'_>,
    left: ArenaBuildOwner,
    right: ArenaBuildOwner,
    receipt: &mut SequenceMutationReceipt,
) -> Result<ArenaBuildOwner, Spec::Error> {
    receipt.nodes_visited = receipt
        .nodes_visited
        .checked_add(1)
        .ok_or_else(|| Spec::invalid("sequence visit count overflow"))?;
    let left_height = owner_height::<Spec>(session, &left, receipt)?;
    let right_height = owner_height::<Spec>(session, &right, receipt)?;
    receipt.maximum_atomic_height = receipt
        .maximum_atomic_height
        .max(left_height.max(right_height));
    if left_height > right_height.saturating_add(1) {
        let (outer, inner) = decompose_branch::<Spec>(session, left, receipt)?;
        let joined = join_owned_atomic::<Spec>(session, inner, right, receipt)?;
        return balance_owned_atomic::<Spec>(session, outer, joined, receipt);
    }
    if right_height > left_height.saturating_add(1) {
        let (inner, outer) = decompose_branch::<Spec>(session, right, receipt)?;
        let joined = join_owned_atomic::<Spec>(session, left, inner, receipt)?;
        return balance_owned_atomic::<Spec>(session, joined, outer, receipt);
    }
    make_branch::<Spec>(session, left, right, receipt)
}

fn balance_owned_atomic<Spec: SequenceSpec>(
    session: &mut ArenaBuildSession<'_>,
    left: ArenaBuildOwner,
    right: ArenaBuildOwner,
    receipt: &mut SequenceMutationReceipt,
) -> Result<ArenaBuildOwner, Spec::Error> {
    let left_height = owner_height::<Spec>(session, &left, receipt)?;
    let right_height = owner_height::<Spec>(session, &right, receipt)?;
    if left_height > right_height.saturating_add(1) {
        let (left_left, left_right) = decompose_branch::<Spec>(session, left, receipt)?;
        if owner_height::<Spec>(session, &left_right, receipt)?
            > owner_height::<Spec>(session, &left_left, receipt)?
        {
            let (pivot_left, pivot_right) = decompose_branch::<Spec>(session, left_right, receipt)?;
            let next_left = make_branch::<Spec>(session, left_left, pivot_left, receipt)?;
            let next_right = make_branch::<Spec>(session, pivot_right, right, receipt)?;
            return make_branch::<Spec>(session, next_left, next_right, receipt);
        }
        let next_right = make_branch::<Spec>(session, left_right, right, receipt)?;
        return make_branch::<Spec>(session, left_left, next_right, receipt);
    }
    if right_height > left_height.saturating_add(1) {
        let (right_left, right_right) = decompose_branch::<Spec>(session, right, receipt)?;
        if owner_height::<Spec>(session, &right_left, receipt)?
            > owner_height::<Spec>(session, &right_right, receipt)?
        {
            let (pivot_left, pivot_right) = decompose_branch::<Spec>(session, right_left, receipt)?;
            let next_left = make_branch::<Spec>(session, left, pivot_left, receipt)?;
            let next_right = make_branch::<Spec>(session, pivot_right, right_right, receipt)?;
            return make_branch::<Spec>(session, next_left, next_right, receipt);
        }
        let next_left = make_branch::<Spec>(session, left, right_left, receipt)?;
        return make_branch::<Spec>(session, next_left, right_right, receipt);
    }
    make_branch::<Spec>(session, left, right, receipt)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ResumableSequenceProgress {
    Pending,
    Complete,
}

struct SequenceReduction<Spec> {
    next_bin: usize,
    root: Option<ArenaBuildOwner>,
    joining: bool,
    marker: PhantomData<Spec>,
}

/// Allocation-granular builder bound to one exact arena journal.
///
/// Every poll validates the build key before mutating local state. It performs
/// no scratch allocation and allocates at most one arena branch. Any error is
/// terminal for the surrounding build; the caller must abort its journal.
pub(crate) struct ResumableMeasuredSequenceBuilder<Spec> {
    build: ArenaBuildKey,
    bins: Vec<Option<ArenaBuildOwner>>,
    bin_capacity: usize,
    carry: Option<(ArenaBuildOwner, usize)>,
    reduction: Option<SequenceReduction<Spec>>,
    join: ResumableSequenceJoin<Spec>,
    poisoned: bool,
    marker: PhantomData<Spec>,
}

impl<Spec: SequenceSpec> ResumableMeasuredSequenceBuilder<Spec> {
    pub(crate) fn try_new(
        session: &mut ArenaBuildSession<'_>,
        receipt: &mut SequenceMutationReceipt,
    ) -> Result<Self, Spec::Error> {
        let owner_reservation =
            MAX_SEQUENCE_BUILDER_OWNER_RESERVATION.min(session.remaining_owner_capacity()?);
        session.reserve_owner_capacity(owner_reservation)?;
        receipt.reserved_owner_slots = receipt.reserved_owner_slots.max(owner_reservation);
        let mut bins = Vec::new();
        bins.try_reserve_exact(MAX_SEQUENCE_BIN_SLOTS)
            .map_err(|_| Spec::invalid("sequence bin reservation failed"))?;
        let bin_capacity = bins.capacity();
        let join = ResumableSequenceJoin::<Spec>::try_new(receipt)?;
        let builder = Self {
            build: session.key(),
            bins,
            bin_capacity,
            carry: None,
            reduction: None,
            join,
            poisoned: false,
            marker: PhantomData,
        };
        builder.record_scratch(receipt);
        Ok(builder)
    }

    pub(crate) fn begin_push(
        &mut self,
        session: &ArenaBuildSession<'_>,
        leaf: ArenaBuildOwner,
        receipt: &mut SequenceMutationReceipt,
    ) -> Result<(), Spec::Error> {
        self.require_session(session)?;
        if self.carry.is_some() || self.reduction.is_some() {
            return Err(Spec::invalid("sequence operation is already active"));
        }
        session.validate_owner(&leaf)?;
        if !matches!(
            sequence_node::<Spec>(session.arena(), leaf.id(), &mut receipt.inspection)?.kind,
            SequenceNodeKind::Leaf
        ) {
            return Err(Spec::invalid("sequence input is not a leaf"));
        }
        receipt.leaves_adopted = receipt
            .leaves_adopted
            .checked_add(1)
            .ok_or_else(|| Spec::invalid("sequence leaf count overflow"))?;
        self.carry = Some((leaf, 0));
        self.record_scratch(receipt);
        Ok(())
    }

    pub(crate) fn poll_push(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
        receipt: &mut SequenceMutationReceipt,
    ) -> Result<ResumableSequenceProgress, Spec::Error> {
        self.require_session(session)?;
        if self.reduction.is_some() {
            return Err(Spec::invalid("sequence finalization already started"));
        }
        let Some((carry, level)) = self.carry.take() else {
            return Ok(ResumableSequenceProgress::Complete);
        };
        self.poisoned = true;
        let result = (|| {
            session.validate_owner(&carry)?;
            if level == self.bins.len() {
                if self.bins.len() >= MAX_SEQUENCE_BIN_SLOTS {
                    return Err(Spec::invalid("sequence exceeded its u64 leaf domain"));
                }
                self.bins.push(Some(carry));
                self.record_scratch(receipt);
                return Ok(ResumableSequenceProgress::Complete);
            }
            let Some(left) = self.bins[level].take() else {
                self.bins[level] = Some(carry);
                self.record_scratch(receipt);
                return Ok(ResumableSequenceProgress::Complete);
            };
            let branch = make_branch::<Spec>(session, left, carry, receipt)?;
            self.carry = Some((branch, level + 1));
            self.record_scratch(receipt);
            Ok(ResumableSequenceProgress::Pending)
        })();
        if result.is_ok() {
            self.poisoned = false;
        }
        result
    }

    pub(crate) fn begin_finish(
        &mut self,
        session: &ArenaBuildSession<'_>,
        receipt: &mut SequenceMutationReceipt,
    ) -> Result<(), Spec::Error> {
        self.require_session(session)?;
        if self.carry.is_some() {
            return Err(Spec::invalid("sequence push is still pending"));
        }
        if self.reduction.is_some() {
            return Err(Spec::invalid("sequence finalization already started"));
        }
        // There is at most one bin per bit of the u64 leaf count. This bounded
        // compaction allocates nothing and restores exact source order.
        self.bins.reverse();
        self.bins.retain(Option::is_some);
        let root = self
            .bins
            .first_mut()
            .and_then(Option::take)
            .ok_or_else(|| Spec::invalid("cannot finish an empty sequence"))?;
        self.reduction = Some(SequenceReduction {
            next_bin: 1,
            root: Some(root),
            joining: false,
            marker: PhantomData,
        });
        self.record_scratch(receipt);
        Ok(())
    }

    pub(crate) fn poll_finish(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
        receipt: &mut SequenceMutationReceipt,
    ) -> Result<ResumableSequenceProgress, Spec::Error> {
        self.require_session(session)?;
        let joining = self
            .reduction
            .as_ref()
            .ok_or_else(|| Spec::invalid("sequence finalization has not started"))?
            .joining;
        self.poisoned = true;
        let result = (|| {
            if joining {
                if self.join.poll(session, receipt)? == ResumableSequenceProgress::Complete {
                    let root = self.join.take_root()?;
                    let reduction = self
                        .reduction
                        .as_mut()
                        .ok_or_else(|| Spec::invalid("sequence reduction disappeared"))?;
                    reduction.root = Some(root);
                    reduction.joining = false;
                }
                self.record_scratch(receipt);
                return Ok(ResumableSequenceProgress::Pending);
            }

            let reduction = self
                .reduction
                .as_mut()
                .ok_or_else(|| Spec::invalid("sequence finalization has not started"))?;
            if reduction.next_bin == self.bins.len() {
                return Ok(ResumableSequenceProgress::Complete);
            }
            let left = reduction
                .root
                .take()
                .ok_or_else(|| Spec::invalid("sequence reduction lost its root"))?;
            let right = self.bins[reduction.next_bin]
                .take()
                .ok_or_else(|| Spec::invalid("sequence reduction lost a bin root"))?;
            reduction.next_bin += 1;
            self.join.begin(session, left, right, receipt)?;
            reduction.joining = true;
            self.record_scratch(receipt);
            Ok(ResumableSequenceProgress::Pending)
        })();
        if result.is_ok() {
            self.poisoned = false;
        }
        result
    }

    pub(crate) fn take_root(
        &mut self,
        session: &ArenaBuildSession<'_>,
    ) -> Result<MeasuredSequenceBuildRoot<Spec>, Spec::Error> {
        self.require_session(session)?;
        let reduction = self
            .reduction
            .as_ref()
            .ok_or_else(|| Spec::invalid("sequence finalization has not started"))?;
        if reduction.next_bin != self.bins.len() || reduction.joining {
            return Err(Spec::invalid("sequence finalization is incomplete"));
        }
        self.poisoned = true;
        let result = (|| {
            let owner = self
                .reduction
                .take()
                .and_then(|reduction| reduction.root)
                .ok_or_else(|| Spec::invalid("sequence finalization lost its root"))?;
            session.validate_owner(&owner)?;
            self.bins.clear();
            Ok(MeasuredSequenceBuildRoot {
                owner,
                marker: PhantomData,
            })
        })();
        if result.is_ok() {
            self.poisoned = false;
        }
        result
    }

    fn require_session(&self, session: &ArenaBuildSession<'_>) -> Result<(), Spec::Error> {
        if self.poisoned {
            return Err(Spec::invalid("sequence builder is poisoned"));
        }
        session.validate_key(self.build)?;
        if self.bins.capacity() != self.bin_capacity {
            return Err(Spec::invalid("sequence bin capacity changed"));
        }
        Ok(())
    }

    fn record_scratch(&self, receipt: &mut SequenceMutationReceipt) {
        let live_bins = self.bins.iter().filter(|value| value.is_some()).count()
            + usize::from(self.carry.is_some());
        receipt.maximum_live_bins = receipt.maximum_live_bins.max(live_bins);
        let bin_bytes = self
            .bins
            .capacity()
            .saturating_mul(std::mem::size_of::<Option<ArenaBuildOwner>>());
        receipt.reserved_scratch_bytes = receipt
            .reserved_scratch_bytes
            .max(bin_bytes.saturating_add(self.join.reserved_scratch_bytes()));
        self.join.record_scratch(receipt);
    }
}

fn make_branch<Spec: SequenceSpec>(
    session: &mut ArenaBuildSession<'_>,
    left: ArenaBuildOwner,
    right: ArenaBuildOwner,
    receipt: &mut SequenceMutationReceipt,
) -> Result<ArenaBuildOwner, Spec::Error> {
    session.validate_owner(&left)?;
    session.validate_owner(&right)?;
    let left_id = left.id();
    let right_id = right.id();
    let left_measure =
        sequence_node::<Spec>(session.arena(), left_id, &mut receipt.inspection)?.measure;
    let right_measure =
        sequence_node::<Spec>(session.arena(), right_id, &mut receipt.inspection)?.measure;
    let measure = combine_measures::<Spec>(left_measure, right_measure, &mut receipt.inspection)?;

    let mut payload = [0_u8; ARENA_PAGE_BYTES];
    let payload_len = Spec::encode_branch(measure, &mut payload)?;
    if payload_len == 0 || payload_len > payload.len() {
        return Err(Spec::invalid("sequence branch encoding length is invalid"));
    }
    let parent = session.allocate(&payload[..payload_len], &[left_id, right_id])?;
    session.release(left)?;
    session.release(right)?;
    receipt.branches_allocated = receipt
        .branches_allocated
        .checked_add(1)
        .ok_or_else(|| Spec::invalid("sequence branch count overflow"))?;
    receipt.branch_payload_bytes = receipt
        .branch_payload_bytes
        .checked_add(payload_len)
        .ok_or_else(|| Spec::invalid("sequence branch byte count overflow"))?;
    Ok(parent)
}

fn owner_height<Spec: SequenceSpec>(
    session: &ArenaBuildSession<'_>,
    owner: &ArenaBuildOwner,
    receipt: &mut SequenceMutationReceipt,
) -> Result<u16, Spec::Error> {
    session.validate_owner(owner)?;
    Ok(
        sequence_node::<Spec>(session.arena(), owner.id(), &mut receipt.inspection)?
            .measure
            .height,
    )
}

fn decompose_branch<Spec: SequenceSpec>(
    session: &mut ArenaBuildSession<'_>,
    node: ArenaBuildOwner,
    receipt: &mut SequenceMutationReceipt,
) -> Result<(ArenaBuildOwner, ArenaBuildOwner), Spec::Error> {
    session.validate_owner(&node)?;
    receipt.nodes_visited = receipt
        .nodes_visited
        .checked_add(1)
        .ok_or_else(|| Spec::invalid("sequence visit count overflow"))?;
    let SequenceNodeKind::Branch { left, right, .. } =
        sequence_node::<Spec>(session.arena(), node.id(), &mut receipt.inspection)?.kind
    else {
        return Err(Spec::invalid("expected a sequence branch"));
    };
    let left = session.retain(left)?;
    let right = session.retain(right)?;
    session.release(node)?;
    Ok((left, right))
}

enum ResumableJoinTask {
    Join {
        left: ArenaBuildOwner,
        right: ArenaBuildOwner,
    },
    Balance {
        left: ArenaBuildOwner,
        right: ArenaBuildOwner,
    },
    BalanceWithLeft {
        left: ArenaBuildOwner,
    },
    BalanceWithRight {
        right: ArenaBuildOwner,
    },
    MakeBranch {
        left: ArenaBuildOwner,
        right: ArenaBuildOwner,
    },
    MakeBranchWithLeft {
        left: ArenaBuildOwner,
    },
    MakeBranchWithRight {
        right: ArenaBuildOwner,
    },
    MakeBranchFromValues,
}

struct ResumableSequenceJoin<Spec> {
    tasks: Vec<ResumableJoinTask>,
    values: Vec<ArenaBuildOwner>,
    task_capacity: usize,
    value_capacity: usize,
    marker: PhantomData<Spec>,
}

impl<Spec: SequenceSpec> ResumableSequenceJoin<Spec> {
    fn try_new(receipt: &mut SequenceMutationReceipt) -> Result<Self, Spec::Error> {
        let task_slots = usize::from(MAX_SEQUENCE_AVL_HEIGHT)
            .checked_add(4)
            .ok_or_else(|| Spec::invalid("sequence join task bound overflow"))?;
        let mut tasks = Vec::new();
        tasks
            .try_reserve_exact(task_slots)
            .map_err(|_| Spec::invalid("sequence join task reservation failed"))?;
        let mut values = Vec::new();
        values
            .try_reserve_exact(2)
            .map_err(|_| Spec::invalid("sequence join value reservation failed"))?;
        let join = Self {
            task_capacity: tasks.capacity(),
            value_capacity: values.capacity(),
            tasks,
            values,
            marker: PhantomData,
        };
        join.record_scratch(receipt);
        Ok(join)
    }

    fn begin(
        &mut self,
        session: &ArenaBuildSession<'_>,
        left: ArenaBuildOwner,
        right: ArenaBuildOwner,
        receipt: &mut SequenceMutationReceipt,
    ) -> Result<(), Spec::Error> {
        self.require_idle()?;
        session.validate_owner(&left)?;
        session.validate_owner(&right)?;
        let height = owner_height::<Spec>(session, &left, receipt)?
            .max(owner_height::<Spec>(session, &right, receipt)?);
        if height > MAX_SEQUENCE_AVL_HEIGHT {
            return Err(Spec::invalid("sequence join exceeds its height bound"));
        }
        self.push_task(ResumableJoinTask::Join { left, right })?;
        self.record_scratch(receipt);
        Ok(())
    }

    fn poll(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
        receipt: &mut SequenceMutationReceipt,
    ) -> Result<ResumableSequenceProgress, Spec::Error> {
        self.require_fixed_capacity()?;
        let Some(task) = self.tasks.pop() else {
            return if self.values.len() == 1 {
                Ok(ResumableSequenceProgress::Complete)
            } else {
                Err(Spec::invalid("sequence join has no complete value"))
            };
        };
        self.execute_task(session, receipt, task)?;
        self.require_fixed_capacity()?;
        self.record_scratch(receipt);
        if self.tasks.is_empty() && self.values.len() == 1 {
            Ok(ResumableSequenceProgress::Complete)
        } else {
            Ok(ResumableSequenceProgress::Pending)
        }
    }

    fn execute_task(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
        receipt: &mut SequenceMutationReceipt,
        task: ResumableJoinTask,
    ) -> Result<(), Spec::Error> {
        match task {
            ResumableJoinTask::Join { left, right } => {
                self.schedule_join(session, receipt, left, right)?;
            }
            ResumableJoinTask::Balance { left, right } => {
                self.schedule_balance(session, receipt, left, right)?;
            }
            ResumableJoinTask::BalanceWithLeft { left } => {
                let right = self.pop_value()?;
                self.push_task(ResumableJoinTask::Balance { left, right })?;
            }
            ResumableJoinTask::BalanceWithRight { right } => {
                let left = self.pop_value()?;
                self.push_task(ResumableJoinTask::Balance { left, right })?;
            }
            ResumableJoinTask::MakeBranch { left, right } => {
                let branch = make_branch::<Spec>(session, left, right, receipt)?;
                self.push_value(branch)?;
            }
            ResumableJoinTask::MakeBranchWithLeft { left } => {
                let right = self.pop_value()?;
                let branch = make_branch::<Spec>(session, left, right, receipt)?;
                self.push_value(branch)?;
            }
            ResumableJoinTask::MakeBranchWithRight { right } => {
                let left = self.pop_value()?;
                let branch = make_branch::<Spec>(session, left, right, receipt)?;
                self.push_value(branch)?;
            }
            ResumableJoinTask::MakeBranchFromValues => {
                let right = self.pop_value()?;
                let left = self.pop_value()?;
                let branch = make_branch::<Spec>(session, left, right, receipt)?;
                self.push_value(branch)?;
            }
        }
        Ok(())
    }

    fn schedule_join(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
        receipt: &mut SequenceMutationReceipt,
        left: ArenaBuildOwner,
        right: ArenaBuildOwner,
    ) -> Result<(), Spec::Error> {
        receipt.nodes_visited = receipt
            .nodes_visited
            .checked_add(1)
            .ok_or_else(|| Spec::invalid("sequence visit count overflow"))?;
        let left_height = owner_height::<Spec>(session, &left, receipt)?;
        let right_height = owner_height::<Spec>(session, &right, receipt)?;
        if left_height > right_height.saturating_add(1) {
            let (outer, inner) = decompose_branch::<Spec>(session, left, receipt)?;
            self.push_task(ResumableJoinTask::BalanceWithLeft { left: outer })?;
            self.push_task(ResumableJoinTask::Join { left: inner, right })?;
        } else if right_height > left_height.saturating_add(1) {
            let (inner, outer) = decompose_branch::<Spec>(session, right, receipt)?;
            self.push_task(ResumableJoinTask::BalanceWithRight { right: outer })?;
            self.push_task(ResumableJoinTask::Join { left, right: inner })?;
        } else {
            self.push_task(ResumableJoinTask::MakeBranch { left, right })?;
        }
        Ok(())
    }

    fn schedule_balance(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
        receipt: &mut SequenceMutationReceipt,
        left: ArenaBuildOwner,
        right: ArenaBuildOwner,
    ) -> Result<(), Spec::Error> {
        let left_height = owner_height::<Spec>(session, &left, receipt)?;
        let right_height = owner_height::<Spec>(session, &right, receipt)?;
        if left_height > right_height.saturating_add(1) {
            let (left_left, left_right) = decompose_branch::<Spec>(session, left, receipt)?;
            if owner_height::<Spec>(session, &left_right, receipt)?
                > owner_height::<Spec>(session, &left_left, receipt)?
            {
                let (pivot_left, pivot_right) =
                    decompose_branch::<Spec>(session, left_right, receipt)?;
                self.push_task(ResumableJoinTask::MakeBranchFromValues)?;
                self.push_task(ResumableJoinTask::MakeBranch {
                    left: pivot_right,
                    right,
                })?;
                self.push_task(ResumableJoinTask::MakeBranch {
                    left: left_left,
                    right: pivot_left,
                })?;
            } else {
                self.push_task(ResumableJoinTask::MakeBranchWithLeft { left: left_left })?;
                self.push_task(ResumableJoinTask::MakeBranch {
                    left: left_right,
                    right,
                })?;
            }
        } else if right_height > left_height.saturating_add(1) {
            let (right_left, right_right) = decompose_branch::<Spec>(session, right, receipt)?;
            if owner_height::<Spec>(session, &right_left, receipt)?
                > owner_height::<Spec>(session, &right_right, receipt)?
            {
                let (pivot_left, pivot_right) =
                    decompose_branch::<Spec>(session, right_left, receipt)?;
                self.push_task(ResumableJoinTask::MakeBranchFromValues)?;
                self.push_task(ResumableJoinTask::MakeBranch {
                    left: pivot_right,
                    right: right_right,
                })?;
                self.push_task(ResumableJoinTask::MakeBranch {
                    left,
                    right: pivot_left,
                })?;
            } else {
                self.push_task(ResumableJoinTask::MakeBranchWithRight { right: right_right })?;
                self.push_task(ResumableJoinTask::MakeBranch {
                    left,
                    right: right_left,
                })?;
            }
        } else {
            self.push_task(ResumableJoinTask::MakeBranch { left, right })?;
        }
        Ok(())
    }

    fn take_root(&mut self) -> Result<ArenaBuildOwner, Spec::Error> {
        if !self.tasks.is_empty() || self.values.len() != 1 {
            return Err(Spec::invalid("sequence join is incomplete"));
        }
        self.values
            .pop()
            .ok_or_else(|| Spec::invalid("sequence join lost its root"))
    }

    fn pop_value(&mut self) -> Result<ArenaBuildOwner, Spec::Error> {
        self.values
            .pop()
            .ok_or_else(|| Spec::invalid("sequence join continuation lost its value"))
    }

    fn push_task(&mut self, task: ResumableJoinTask) -> Result<(), Spec::Error> {
        if self.tasks.len() >= self.task_capacity {
            return Err(Spec::invalid("sequence join exceeded its task bound"));
        }
        self.tasks.push(task);
        Ok(())
    }

    fn push_value(&mut self, value: ArenaBuildOwner) -> Result<(), Spec::Error> {
        if self.values.len() >= self.value_capacity {
            return Err(Spec::invalid("sequence join exceeded its value bound"));
        }
        self.values.push(value);
        Ok(())
    }

    fn require_idle(&self) -> Result<(), Spec::Error> {
        self.require_fixed_capacity()?;
        if !self.tasks.is_empty() || !self.values.is_empty() {
            return Err(Spec::invalid("sequence join is already active"));
        }
        Ok(())
    }

    fn require_fixed_capacity(&self) -> Result<(), Spec::Error> {
        if self.tasks.capacity() != self.task_capacity
            || self.values.capacity() != self.value_capacity
        {
            return Err(Spec::invalid("sequence join scratch capacity changed"));
        }
        Ok(())
    }

    fn reserved_scratch_bytes(&self) -> usize {
        self.tasks
            .capacity()
            .saturating_mul(std::mem::size_of::<ResumableJoinTask>())
            .saturating_add(
                self.values
                    .capacity()
                    .saturating_mul(std::mem::size_of::<ArenaBuildOwner>()),
            )
    }

    fn record_scratch(&self, receipt: &mut SequenceMutationReceipt) {
        receipt.maximum_join_tasks = receipt.maximum_join_tasks.max(self.tasks.len());
        receipt.maximum_join_values = receipt.maximum_join_values.max(self.values.len());
        receipt.reserved_scratch_bytes = receipt
            .reserved_scratch_bytes
            .max(self.reserved_scratch_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::ArenaLimits;

    const LEAF_TAG: u8 = 0xa1;
    const BRANCH_TAG: u8 = 0xa2;

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    struct TestSummary {
        sum: u64,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum TestError {
        Arena(ArenaError),
        Invalid(&'static str),
    }

    impl From<ArenaError> for TestError {
        fn from(value: ArenaError) -> Self {
            Self::Arena(value)
        }
    }

    struct TestSpec;

    impl SequenceSpec for TestSpec {
        type Summary = TestSummary;
        type Error = TestError;

        fn leaf_summary(
            payload: &[u8],
            _inspection: &mut SequenceSpecInspection,
        ) -> Result<Option<Self::Summary>, Self::Error> {
            if payload.first().copied() != Some(LEAF_TAG) {
                return Ok(None);
            }
            if payload.len() != 9 {
                return Err(TestError::Invalid("malformed test leaf"));
            }
            Ok(Some(TestSummary {
                sum: u64::from_le_bytes(payload[1..9].try_into().expect("fixed leaf")),
            }))
        }

        fn branch_measure(
            payload: &[u8],
            _inspection: &mut SequenceSpecInspection,
        ) -> Result<Option<SequenceMeasure<Self::Summary>>, Self::Error> {
            if payload.first().copied() != Some(BRANCH_TAG) {
                return Ok(None);
            }
            if payload.len() != 19 {
                return Err(TestError::Invalid("malformed test branch"));
            }
            Ok(Some(SequenceMeasure {
                leaves: u64::from_le_bytes(payload[1..9].try_into().expect("fixed leaves")),
                height: u16::from_le_bytes(payload[9..11].try_into().expect("fixed height")),
                summary: TestSummary {
                    sum: u64::from_le_bytes(payload[11..19].try_into().expect("fixed sum")),
                },
            }))
        }

        fn encode_branch(
            measure: SequenceMeasure<Self::Summary>,
            output: &mut [u8; ARENA_PAGE_BYTES],
        ) -> Result<usize, Self::Error> {
            output[0] = BRANCH_TAG;
            output[1..9].copy_from_slice(&measure.leaves.to_le_bytes());
            output[9..11].copy_from_slice(&measure.height.to_le_bytes());
            output[11..19].copy_from_slice(&measure.summary.sum.to_le_bytes());
            Ok(19)
        }

        fn combine(
            left: Self::Summary,
            right: Self::Summary,
        ) -> Result<Self::Summary, Self::Error> {
            Ok(TestSummary {
                sum: left
                    .sum
                    .checked_add(right.sum)
                    .ok_or(TestError::Invalid("test sum overflow"))?,
            })
        }

        fn invalid(message: &'static str) -> Self::Error {
            TestError::Invalid(message)
        }
    }

    struct RejectingSpec;

    impl SequenceSpec for RejectingSpec {
        type Summary = TestSummary;
        type Error = TestError;

        fn leaf_summary(
            _payload: &[u8],
            _inspection: &mut SequenceSpecInspection,
        ) -> Result<Option<Self::Summary>, Self::Error> {
            Ok(None)
        }

        fn branch_measure(
            _payload: &[u8],
            _inspection: &mut SequenceSpecInspection,
        ) -> Result<Option<SequenceMeasure<Self::Summary>>, Self::Error> {
            Ok(None)
        }

        fn encode_branch(
            _measure: SequenceMeasure<Self::Summary>,
            _output: &mut [u8; ARENA_PAGE_BYTES],
        ) -> Result<usize, Self::Error> {
            Err(TestError::Invalid("rejecting spec cannot encode"))
        }

        fn combine(
            _left: Self::Summary,
            _right: Self::Summary,
        ) -> Result<Self::Summary, Self::Error> {
            Err(TestError::Invalid("rejecting spec cannot combine"))
        }

        fn invalid(message: &'static str) -> Self::Error {
            TestError::Invalid(message)
        }
    }

    fn limits(max_slots: usize) -> ArenaLimits {
        ArenaLimits {
            max_slots,
            max_live_payload_bytes: max_slots * ARENA_PAGE_BYTES,
            max_children_per_node: 4,
        }
    }

    fn leaf_payload(value: u64) -> [u8; 9] {
        let mut payload = [0_u8; 9];
        payload[0] = LEAF_TAG;
        payload[1..].copy_from_slice(&value.to_le_bytes());
        payload
    }

    fn branch_payload(leaves: u64, height: u16, sum: u64) -> [u8; 19] {
        let mut payload = [0_u8; 19];
        payload[0] = BRANCH_TAG;
        payload[1..9].copy_from_slice(&leaves.to_le_bytes());
        payload[9..11].copy_from_slice(&height.to_le_bytes());
        payload[11..19].copy_from_slice(&sum.to_le_bytes());
        payload
    }

    fn drain(arena: &mut PageArena) {
        while arena.metrics().pending_reclaims != 0 || arena.metrics().pending_build_aborts != 0 {
            let receipt = arena.poll_reclaim(1);
            assert!(receipt.transitions <= 1);
        }
    }

    fn assert_poll_inspection_bound(
        before: SequenceInspectionReceipt,
        after: SequenceInspectionReceipt,
    ) {
        let node_headers = after
            .node_headers_decoded
            .checked_sub(before.node_headers_decoded)
            .expect("node-header inspection count is monotonic");
        let payload_bytes = after
            .spec
            .payload_bytes_inspected
            .checked_sub(before.spec.payload_bytes_inspected)
            .expect("payload inspection count is monotonic");
        let summary_combinations = after
            .summary_combinations
            .checked_sub(before.summary_combinations)
            .expect("summary-combination count is monotonic");
        let spec_items_hashed = after
            .spec
            .spec_items_hashed
            .checked_sub(before.spec.spec_items_hashed)
            .expect("spec hash-item count is monotonic");

        assert!(node_headers <= MAX_SEQUENCE_POLL_NODE_HEADERS);
        assert!(payload_bytes <= MAX_SEQUENCE_POLL_PAYLOAD_BYTES);
        assert!(summary_combinations <= MAX_SEQUENCE_POLL_SUMMARY_COMBINATIONS);
        assert_eq!(
            spec_items_hashed, 0,
            "the scalar test spec performs no payload hashing"
        );
    }

    fn seal<Spec>(
        arena: &mut PageArena,
        build: CandidateBuild,
        root: MeasuredSequenceBuildRoot<Spec>,
    ) -> CommittedMeasuredSequenceRoot<Spec> {
        let mut seal = match begin_measured_sequence_seal(arena, build, root) {
            Ok(seal) => seal,
            Err(_) => panic!("begin measured seal"),
        };
        loop {
            let poll = seal.poll(arena, 1).expect("poll measured seal");
            if let Some(root) = poll.root {
                return root;
            }
        }
    }

    fn four_leaf_tree(arena: &mut PageArena) -> CommittedMeasuredSequenceRoot<TestSpec> {
        let (build, root) = {
            let mut session = arena.begin_build().expect("build");
            let one = session.allocate(&leaf_payload(1), &[]).expect("leaf one");
            let two = session.allocate(&leaf_payload(2), &[]).expect("leaf two");
            let left = session
                .allocate(&branch_payload(2, 2, 3), &[one.id(), two.id()])
                .expect("left branch");
            session.release(one).expect("release one owner");
            session.release(two).expect("release two owner");

            let three = session.allocate(&leaf_payload(3), &[]).expect("leaf three");
            let four = session.allocate(&leaf_payload(4), &[]).expect("leaf four");
            let right = session
                .allocate(&branch_payload(2, 2, 7), &[three.id(), four.id()])
                .expect("right branch");
            session.release(three).expect("release three owner");
            session.release(four).expect("release four owner");

            let root = session
                .allocate(&branch_payload(4, 3, 10), &[left.id(), right.id()])
                .expect("root branch");
            session.release(left).expect("release left owner");
            session.release(right).expect("release right owner");
            let build = session.suspend().expect("suspend");
            (
                build,
                MeasuredSequenceBuildRoot {
                    owner: root,
                    marker: PhantomData,
                },
            )
        };
        seal(arena, build, root)
    }

    fn armed_multi_owner_seal(arena: &mut PageArena) -> MeasuredSequenceSeal<TestSpec> {
        let (build, root) = {
            let mut session = arena.begin_build().expect("multi-owner build");
            for value in 1..4 {
                session
                    .allocate(&leaf_payload(value), &[])
                    .expect("non-root owner");
            }
            let root = session
                .allocate(&leaf_payload(4), &[])
                .expect("latest root owner");
            let build = session.suspend().expect("suspend multi-owner build");
            (
                build,
                MeasuredSequenceBuildRoot {
                    owner: root,
                    marker: PhantomData,
                },
            )
        };
        match begin_measured_sequence_seal(arena, build, root) {
            Ok(seal) => seal,
            Err(_) => panic!("begin multi-owner seal"),
        }
    }

    fn build_values(
        arena: &mut PageArena,
        values: &[u64],
    ) -> (
        CommittedMeasuredSequenceRoot<TestSpec>,
        SequenceMutationReceipt,
    ) {
        let mut receipt = SequenceMutationReceipt::default();
        let mut session = arena.begin_build().expect("build");
        let mut builder =
            ResumableMeasuredSequenceBuilder::<TestSpec>::try_new(&mut session, &mut receipt)
                .expect("builder");
        for &value in values {
            let leaf = session
                .allocate(&leaf_payload(value), &[])
                .expect("allocate leaf");
            builder
                .begin_push(&session, leaf, &mut receipt)
                .expect("begin push");
            loop {
                let branches = receipt.branches_allocated;
                let inspection = receipt.inspection;
                let progress = builder
                    .poll_push(&mut session, &mut receipt)
                    .expect("poll push");
                assert!(receipt.branches_allocated - branches <= 1);
                assert_poll_inspection_bound(inspection, receipt.inspection);
                let suspended = session.suspend().expect("suspend push");
                session = arena.resume_build(suspended).expect("resume push");
                if progress == ResumableSequenceProgress::Complete {
                    break;
                }
            }
        }
        builder
            .begin_finish(&session, &mut receipt)
            .expect("begin finish");
        loop {
            let branches = receipt.branches_allocated;
            let inspection = receipt.inspection;
            let progress = builder
                .poll_finish(&mut session, &mut receipt)
                .expect("poll finish");
            assert!(receipt.branches_allocated - branches <= 1);
            assert_poll_inspection_bound(inspection, receipt.inspection);
            let suspended = session.suspend().expect("suspend finish");
            session = arena.resume_build(suspended).expect("resume finish");
            if progress == ResumableSequenceProgress::Complete {
                break;
            }
        }
        let root = builder.take_root(&session).expect("take root");
        let build = session.suspend().expect("suspend root");
        (seal(arena, build, root), receipt)
    }

    fn maximum_height_shared_tree(
        arena: &mut PageArena,
    ) -> CommittedMeasuredSequenceRoot<TestSpec> {
        let mut receipt = SequenceMutationReceipt::default();
        let mut session = arena.begin_build().expect("maximum-height build");
        let leaf = session.allocate(&leaf_payload(0), &[]).expect("zero leaf");
        let mut levels = vec![(leaf.id(), 1_u64)];
        let mut root = leaf;
        for height in 2..=MAX_SEQUENCE_AVL_HEIGHT {
            let left = levels[usize::from(height - 2)];
            let right = levels[if height == 2 {
                0
            } else {
                usize::from(height - 3)
            }];
            let leaves = left.1.checked_add(right.1).expect("u64 AVL domain");
            root = session
                .allocate(&branch_payload(leaves, height, 0), &[left.0, right.0])
                .expect("shared maximum-height branch");
            levels.push((root.id(), leaves));
        }
        assert_eq!(
            levels.last().copied().expect("root level").1,
            AVL_MIN_LEAVES_BY_HEIGHT[usize::from(MAX_SEQUENCE_AVL_HEIGHT)]
        );
        let root = validate_measured_sequence_build_owner::<TestSpec>(&session, root, &mut receipt)
            .expect("validated maximum-height root");
        let build = session.suspend().expect("suspend maximum-height build");
        seal(arena, build, root)
    }

    fn atomic_splice_work_units(
        before: SequenceMutationReceipt,
        after: SequenceMutationReceipt,
    ) -> u64 {
        after
            .inspection
            .node_headers_decoded
            .checked_sub(before.inspection.node_headers_decoded)
            .expect("node headers are monotonic")
            .checked_add(
                after
                    .inspection
                    .summary_combinations
                    .checked_sub(before.inspection.summary_combinations)
                    .expect("summary combinations are monotonic"),
            )
            .and_then(|work| {
                work.checked_add(
                    u64::try_from(
                        after
                            .nodes_visited
                            .checked_sub(before.nodes_visited)
                            .expect("node visits are monotonic"),
                    )
                    .expect("node visits fit u64"),
                )
            })
            .and_then(|work| {
                work.checked_add(
                    u64::try_from(
                        after
                            .branches_allocated
                            .checked_sub(before.branches_allocated)
                            .expect("branch allocations are monotonic"),
                    )
                    .expect("branch allocations fit u64"),
                )
            })
            .expect("atomic splice work fits u64")
    }

    fn assert_exact_tree(
        arena: &PageArena,
        id: ArenaId,
        expected_values: &[u64],
        cursor: &mut usize,
        inspection: &mut SequenceInspectionReceipt,
    ) -> SequenceMeasure<TestSummary> {
        let node = sequence_node::<TestSpec>(arena, id, inspection).expect("valid sequence node");
        match node.kind {
            SequenceNodeKind::Leaf => {
                let expected = expected_values[*cursor];
                assert_eq!(arena.payload(id), Ok(&leaf_payload(expected)[..]));
                assert_eq!(node.measure.summary.sum, expected);
                assert_eq!(node.measure.leaves, 1);
                assert_eq!(node.measure.height, 1);
                *cursor += 1;
            }
            SequenceNodeKind::Branch {
                left,
                left_measure,
                right,
                right_measure,
            } => {
                let observed_left =
                    assert_exact_tree(arena, left, expected_values, cursor, inspection);
                let observed_right =
                    assert_exact_tree(arena, right, expected_values, cursor, inspection);
                assert_eq!(left_measure, observed_left);
                assert_eq!(right_measure, observed_right);
                assert!(left_measure.height.abs_diff(right_measure.height) <= 1);
                assert_eq!(
                    node.measure,
                    combine_measures::<TestSpec>(left_measure, right_measure, inspection)
                        .expect("valid combined measure")
                );
            }
        }
        node.measure
    }

    #[test]
    fn node_header_fuel_is_exact_sticky_and_precedes_payload_access() {
        assert_eq!(SequenceInspectionReceipt::with_node_header_limit(0), None);

        let mut arena = PageArena::new(limits(64)).expect("arena");
        let root = four_leaf_tree(&mut arena);
        let root_id = root.root_id_for_test().expect("root id");

        let mut exact = SequenceInspectionReceipt::with_node_header_limit(3)
            .expect("positive exact header limit");
        let decoded = sequence_node::<TestSpec>(&arena, root_id, &mut exact)
            .expect("three-header branch decode");
        assert_eq!(decoded.measure.leaves, 4);
        assert_eq!(exact.node_headers_decoded, 3);
        assert!(!exact.node_header_limit_exhausted());

        let mut short = SequenceInspectionReceipt::with_node_header_limit(2)
            .expect("positive short header limit");
        assert!(matches!(
            sequence_node::<TestSpec>(&arena, root_id, &mut short),
            Err(TestError::Invalid(
                "sequence node-header inspection limit exhausted"
            ))
        ));
        assert_eq!(short.node_headers_decoded, 2);
        assert!(short.node_header_limit_exhausted());
        let inspected_payload_bytes = short.spec.payload_bytes_inspected;

        let mut unlimited = SequenceInspectionReceipt::default();
        sequence_node::<TestSpec>(&arena, root_id, &mut unlimited)
            .expect("default inspection remains unlimited");
        sequence_node::<TestSpec>(&arena, root_id, &mut unlimited)
            .expect("unlimited inspection can continue");
        assert_eq!(unlimited.node_headers_decoded, 6);
        assert!(!unlimited.node_header_limit_exhausted());

        assert!(root.release(&mut arena).is_ok());
        drain(&mut arena);
        assert_eq!(arena.metrics().resident_nodes, 0);

        // The exhausted receipt must refuse before touching even a now-stale
        // arena ID. An arena lookup here would return `TestError::Arena`.
        assert!(matches!(
            sequence_node_header::<TestSpec>(&arena, root_id, &mut short),
            Err(TestError::Invalid(
                "sequence node-header inspection limit exhausted"
            ))
        ));
        assert_eq!(short.node_headers_decoded, 2);
        assert_eq!(short.spec.payload_bytes_inspected, inspected_payload_bytes);
        assert!(short.node_header_limit_exhausted());
    }

    #[test]
    fn summary_and_prefix_routing_keep_semantics_separate_from_shape() {
        let mut arena = PageArena::new(limits(64)).expect("arena");
        let root = four_leaf_tree(&mut arena);
        let sequence = root.as_ref();
        assert_eq!(
            sequence.summary(&arena, &mut SequenceInspectionReceipt::default()),
            Ok(Some(SequenceMeasure {
                summary: TestSummary { sum: 10 },
                leaves: 4,
                height: 3,
            }))
        );

        let expected = [(1, None), (2, Some(1)), (3, Some(3)), (4, Some(6))];
        for (index, (value, prefix_sum)) in expected.into_iter().enumerate() {
            let located = sequence
                .locate_leaf_with_prefix(
                    &arena,
                    index as u64,
                    &mut SequenceInspectionReceipt::default(),
                )
                .expect("locate")
                .expect("present leaf");
            assert_eq!(located.ordinal, index as u64);
            assert_eq!(located.summary.sum, value);
            assert_eq!(located.prefix.map(|summary| summary.sum), prefix_sum);
            assert_eq!(arena.payload(located.id), Ok(&leaf_payload(value)[..]));
        }
        assert_eq!(
            sequence.locate_leaf_with_prefix(&arena, 4, &mut SequenceInspectionReceipt::default(),),
            Ok(None)
        );

        let weighted = [
            (0, 0, 1, None),
            (1, 1, 2, Some(1)),
            (2, 1, 2, Some(1)),
            (3, 2, 3, Some(3)),
            (5, 2, 3, Some(3)),
            (6, 3, 4, Some(6)),
            (9, 3, 4, Some(6)),
        ];
        for (position, ordinal, value, prefix_sum) in weighted {
            let located = sequence
                .locate_leaf_containing_metric(
                    &arena,
                    position,
                    |summary| summary.sum,
                    &mut SequenceInspectionReceipt::default(),
                )
                .expect("metric locate")
                .expect("metric leaf");
            assert_eq!(located.ordinal, ordinal);
            assert_eq!(located.summary.sum, value);
            assert_eq!(located.prefix.map(|summary| summary.sum), prefix_sum);
        }
        assert_eq!(
            sequence.locate_leaf_containing_metric(
                &arena,
                10,
                |summary| summary.sum,
                &mut SequenceInspectionReceipt::default(),
            ),
            Ok(None)
        );
        assert!(matches!(
            sequence.locate_leaf_containing_metric(
                &arena,
                0,
                |_| 1,
                &mut SequenceInspectionReceipt::default(),
            ),
            Err(TestError::Invalid(
                "sequence metric projection is not additive"
            ))
        ));

        assert!(root.release(&mut arena).is_ok());
        drain(&mut arena);
        assert_eq!(arena.metrics().resident_nodes, 0);
    }

    #[test]
    fn resumable_builder_is_avl_exact_and_allocates_at_most_one_branch_per_poll() {
        let mut arena = PageArena::new(limits(1024)).expect("arena");
        let values = (1..=257).collect::<Vec<_>>();
        let (root, receipt) = build_values(&mut arena, &values);
        assert_eq!(receipt.leaves_adopted, values.len());
        // Fresh binomial carries allocate N-1 branches; the final AVL join may
        // path-copy a logarithmic number of them while balancing the tail.
        assert!(receipt.branches_allocated >= values.len() - 1);
        assert!(receipt.branches_allocated <= values.len() - 1 + 2 * MAX_SEQUENCE_BIN_SLOTS);
        assert!(receipt.maximum_live_bins <= MAX_SEQUENCE_BIN_SLOTS);
        assert!(receipt.maximum_join_values <= 2);

        let sequence = root.as_ref();
        let measure = sequence
            .summary(&arena, &mut SequenceInspectionReceipt::default())
            .expect("summary")
            .expect("root");
        assert_eq!(measure.leaves, 257);
        assert_eq!(measure.summary.sum, values.iter().sum());
        assert!(measure.height <= maximum_avl_height(measure.leaves));
        for index in [0_u64, 1, 127, 128, 255, 256] {
            let located = sequence
                .locate_leaf_with_prefix(&arena, index, &mut SequenceInspectionReceipt::default())
                .expect("locate")
                .expect("leaf");
            assert_eq!(located.summary.sum, index + 1);
            assert_eq!(
                located.prefix.map(|summary| summary.sum).unwrap_or(0),
                index * (index + 1) / 2
            );
        }

        assert!(root.release(&mut arena).is_ok());
        drain(&mut arena);
        assert_eq!(arena.metrics().resident_nodes, 0);
    }

    #[test]
    fn every_small_shape_and_large_boundaries_are_avl_exact() {
        let mut counts = (1_usize..=128).collect::<Vec<_>>();
        counts.extend([
            129, 144, 233, 256, 257, 377, 512, 610, 987, 1_024, 1_597, 2_048, 2_584, 4_096,
        ]);
        counts.sort_unstable();
        counts.dedup();

        for count in counts {
            let mut arena = PageArena::new(limits(count * 4 + 256)).expect("arena");
            let values = (1..=u64::try_from(count).expect("count fits u64")).collect::<Vec<_>>();
            let (root, _) = build_values(&mut arena, &values);
            let root_id = root.as_ref().root.expect("root id");
            let mut cursor = 0;
            let mut inspection = SequenceInspectionReceipt::default();
            let measure = assert_exact_tree(&arena, root_id, &values, &mut cursor, &mut inspection);
            assert_eq!(cursor, count, "visited every leaf for count {count}");
            assert_eq!(measure.leaves, count as u64);
            assert_eq!(measure.summary.sum, values.iter().sum());
            assert!(measure.height <= maximum_avl_height(measure.leaves));

            assert!(root.release(&mut arena).is_ok());
            drain(&mut arena);
            assert_eq!(arena.metrics().resident_nodes, 0);
        }
    }

    #[test]
    fn atomic_splice_reuses_untouched_leaf_identity_and_reclaims_shared_roots() {
        let mut arena = PageArena::new(limits(2_048)).expect("arena");
        let values = (1..=257).collect::<Vec<_>>();
        let (base, _) = build_values(&mut arena, &values);
        let base_sequence = base.as_ref();
        let old_ids = [0_u64, 127, 128, 129, 256].map(|index| {
            base_sequence
                .locate_leaf_with_prefix(&arena, index, &mut SequenceInspectionReceipt::default())
                .expect("locate base")
                .expect("base leaf")
                .id
        });

        let mut receipt = SequenceMutationReceipt::default();
        let (build, updated_root) = {
            let mut session = arena.begin_build().expect("splice build");
            let base_root =
                retain_committed_measured_sequence_root(&mut session, &base, &mut receipt)
                    .expect("retain base root");
            let first = session
                .allocate(&leaf_payload(9_001), &[])
                .expect("first replacement leaf");
            let second = session
                .allocate(&leaf_payload(9_002), &[])
                .expect("second replacement leaf");
            let replacement = make_branch::<TestSpec>(&mut session, first, second, &mut receipt)
                .expect("replacement branch");
            let replacement = validate_measured_sequence_build_owner::<TestSpec>(
                &session,
                replacement,
                &mut receipt,
            )
            .expect("validate replacement");
            let root = splice_measured_sequence_build_root_atomic::<TestSpec>(
                &mut session,
                base_root,
                128..129,
                Some(replacement),
                &mut receipt,
            )
            .expect("splice")
            .expect("nonempty splice root");
            (session.suspend().expect("suspend splice"), root)
        };
        let updated = seal(&mut arena, build, updated_root);
        let updated_sequence = updated.as_ref();
        let expected = values[..128]
            .iter()
            .copied()
            .chain([9_001, 9_002])
            .chain(values[129..].iter().copied())
            .collect::<Vec<_>>();
        let measure = updated_sequence
            .summary(&arena, &mut SequenceInspectionReceipt::default())
            .expect("updated summary")
            .expect("updated root");
        assert_eq!(measure.leaves, 258);
        assert_eq!(measure.summary.sum, expected.iter().sum());
        for (index, value) in expected.iter().copied().enumerate() {
            let located = updated_sequence
                .locate_leaf_with_prefix(
                    &arena,
                    index as u64,
                    &mut SequenceInspectionReceipt::default(),
                )
                .expect("locate updated")
                .expect("updated leaf");
            assert_eq!(located.summary.sum, value);
        }

        assert_eq!(
            updated_sequence
                .locate_leaf_with_prefix(&arena, 0, &mut SequenceInspectionReceipt::default(),)
                .expect("first")
                .expect("first leaf")
                .id,
            old_ids[0]
        );
        assert_eq!(
            updated_sequence
                .locate_leaf_with_prefix(&arena, 127, &mut SequenceInspectionReceipt::default(),)
                .expect("left edge")
                .expect("left leaf")
                .id,
            old_ids[1]
        );
        assert_ne!(
            updated_sequence
                .locate_leaf_with_prefix(&arena, 128, &mut SequenceInspectionReceipt::default(),)
                .expect("replacement")
                .expect("replacement leaf")
                .id,
            old_ids[2]
        );
        assert_eq!(
            updated_sequence
                .locate_leaf_with_prefix(&arena, 130, &mut SequenceInspectionReceipt::default(),)
                .expect("right edge")
                .expect("right leaf")
                .id,
            old_ids[3]
        );
        assert_eq!(
            updated_sequence
                .locate_leaf_with_prefix(&arena, 257, &mut SequenceInspectionReceipt::default(),)
                .expect("last")
                .expect("last leaf")
                .id,
            old_ids[4]
        );
        assert_eq!(receipt.leaves_deleted, 1);
        assert_eq!(receipt.leaves_reused, 256);
        assert!(receipt.maximum_atomic_height <= maximum_avl_height(257));
        assert!(receipt.nodes_visited < 128);

        assert!(updated.release(&mut arena).is_ok());
        assert!(base.release(&mut arena).is_ok());
        drain(&mut arena);
        assert_eq!(arena.metrics().resident_nodes, 0);
    }

    #[test]
    fn atomic_splice_noop_shares_exact_root_and_invalid_range_aborts_retained_owner() {
        let mut arena = PageArena::new(limits(256)).expect("arena");
        let values = (1..=32).collect::<Vec<_>>();
        let (base, _) = build_values(&mut arena, &values);
        let base_id = base.as_ref().root.expect("base root");
        let resident_before = arena.metrics().resident_nodes;

        let mut invalid_receipt = SequenceMutationReceipt::default();
        let invalid_build = {
            let mut session = arena.begin_build().expect("invalid build");
            assert!(matches!(
                splice_measured_sequence_atomic::<TestSpec>(
                    &mut session,
                    &base,
                    33..34,
                    None,
                    &mut invalid_receipt,
                ),
                Err(TestError::Invalid("sequence splice range is invalid"))
            ));
            session.suspend().expect("suspend failed splice")
        };
        arena
            .abort_build(invalid_build)
            .expect("abort failed splice");
        drain(&mut arena);
        assert_eq!(arena.metrics().resident_nodes, resident_before);
        assert_eq!(
            invalid_receipt,
            SequenceMutationReceipt {
                inspection: SequenceInspectionReceipt {
                    node_headers_decoded: 6,
                    summary_combinations: 2,
                    spec: SequenceSpecInspection {
                        payload_bytes_inspected: 114,
                        spec_items_hashed: 0,
                    },
                    ..SequenceInspectionReceipt::default()
                },
                committed_leaves_retained: 32,
                reserved_owner_slots: 1,
                ..SequenceMutationReceipt::default()
            }
        );

        let mut noop_receipt = SequenceMutationReceipt::default();
        let (build, shared_root) = {
            let mut session = arena.begin_build().expect("noop build");
            let base_root =
                retain_committed_measured_sequence_root(&mut session, &base, &mut noop_receipt)
                    .expect("retain noop base");
            let root = splice_measured_sequence_build_root_atomic::<TestSpec>(
                &mut session,
                base_root,
                17..17,
                None,
                &mut noop_receipt,
            )
            .expect("noop splice")
            .expect("shared root");
            (session.suspend().expect("suspend noop"), root)
        };
        let shared = seal(&mut arena, build, shared_root);
        assert_eq!(shared.as_ref().root, Some(base_id));
        assert_eq!(noop_receipt.leaves_reused, 32);
        assert_eq!(noop_receipt.committed_leaves_retained, 32);
        assert_eq!(noop_receipt.branches_allocated, 0);

        assert!(shared.release(&mut arena).is_ok());
        assert!(base.release(&mut arena).is_ok());
        drain(&mut arena);
        assert_eq!(arena.metrics().resident_nodes, 0);
    }

    #[test]
    fn build_owner_adapter_rejects_wrong_spec_and_corrupt_payload_without_losing_journal() {
        let mut arena = PageArena::new(limits(32)).expect("arena");
        let build = {
            let mut session = arena.begin_build().expect("adapter build");
            let wrong_spec = session
                .allocate(&leaf_payload(1), &[])
                .expect("wrong-spec leaf");
            assert!(matches!(
                validate_measured_sequence_build_owner::<RejectingSpec>(
                    &session,
                    wrong_spec,
                    &mut SequenceMutationReceipt::default(),
                ),
                Err(TestError::Invalid("unknown sequence node encoding"))
            ));

            let corrupt = session.allocate(&[LEAF_TAG, 0], &[]).expect("corrupt leaf");
            assert!(matches!(
                validate_measured_sequence_build_owner::<TestSpec>(
                    &session,
                    corrupt,
                    &mut SequenceMutationReceipt::default(),
                ),
                Err(TestError::Invalid("malformed test leaf"))
            ));
            session.suspend().expect("suspend rejected owners")
        };
        assert_eq!(arena.metrics().resident_nodes, 2);
        arena.abort_build(build).expect("abort rejected owners");
        drain(&mut arena);
        assert_eq!(arena.metrics().resident_nodes, 0);
        assert_eq!(arena.metrics().live_builds, 0);
    }

    #[test]
    fn abort_after_owned_root_splice_reclaims_every_staged_node_and_preserves_base() {
        let mut arena = PageArena::new(limits(1_024)).expect("arena");
        let values = (1..=64).collect::<Vec<_>>();
        let (base, _) = build_values(&mut arena, &values);
        let resident_base_nodes = arena.metrics().resident_nodes;
        let mut receipt = SequenceMutationReceipt::default();
        let build = {
            let mut session = arena.begin_build().expect("splice build");
            let base_root =
                retain_committed_measured_sequence_root(&mut session, &base, &mut receipt)
                    .expect("retain base");
            let replacement_owner = session
                .allocate(&leaf_payload(9_001), &[])
                .expect("replacement leaf");
            let replacement = validate_measured_sequence_build_owner::<TestSpec>(
                &session,
                replacement_owner,
                &mut receipt,
            )
            .expect("validate replacement");
            let _staged = splice_measured_sequence_build_root_atomic::<TestSpec>(
                &mut session,
                base_root,
                31..33,
                Some(replacement),
                &mut receipt,
            )
            .expect("splice")
            .expect("staged root");
            session.suspend().expect("suspend staged splice")
        };
        assert!(arena.metrics().resident_nodes > resident_base_nodes);
        arena.abort_build(build).expect("abort staged splice");
        drain(&mut arena);
        assert_eq!(arena.metrics().resident_nodes, resident_base_nodes);
        assert_eq!(
            base.as_ref()
                .summary(&arena, &mut SequenceInspectionReceipt::default())
                .expect("base summary")
                .expect("base root")
                .summary
                .sum,
            values.iter().sum()
        );

        assert!(base.release(&mut arena).is_ok());
        drain(&mut arena);
        assert_eq!(arena.metrics().resident_nodes, 0);
    }

    #[test]
    fn atomic_splice_stays_within_named_work_bound_at_authenticated_height_92() {
        let mut arena = PageArena::new(limits(8_192)).expect("arena");
        let base = maximum_height_shared_tree(&mut arena);
        let base_measure = base
            .as_ref()
            .summary(&arena, &mut SequenceInspectionReceipt::default())
            .expect("maximum-height summary")
            .expect("maximum-height root");
        assert_eq!(base_measure.height, MAX_SEQUENCE_AVL_HEIGHT);
        assert_eq!(
            base_measure.leaves,
            AVL_MIN_LEAVES_BY_HEIGHT[usize::from(MAX_SEQUENCE_AVL_HEIGHT)]
        );
        let resident_base_nodes = arena.metrics().resident_nodes;

        let mut boundaries = vec![0, 1, base_measure.leaves / 2, base_measure.leaves - 1];
        for &minimum in &AVL_MIN_LEAVES_BY_HEIGHT[1..] {
            boundaries.push(minimum - 1);
            boundaries.push(base_measure.leaves - minimum);
        }
        boundaries.sort_unstable();
        boundaries.dedup();

        let mut maximum_observed_work = 0;
        for start in boundaries {
            let mut receipt = SequenceMutationReceipt::default();
            let build = {
                let mut session = arena.begin_build().expect("bounded splice build");
                let base_root =
                    retain_committed_measured_sequence_root(&mut session, &base, &mut receipt)
                        .expect("retain maximum-height base");
                let replacement_owner = session
                    .allocate(&leaf_payload(0), &[])
                    .expect("replacement leaf");
                let replacement = validate_measured_sequence_build_owner::<TestSpec>(
                    &session,
                    replacement_owner,
                    &mut receipt,
                )
                .expect("validate replacement");
                let before = receipt;
                let _staged = splice_measured_sequence_build_root_atomic::<TestSpec>(
                    &mut session,
                    base_root,
                    start..start + 1,
                    Some(replacement),
                    &mut receipt,
                )
                .expect("bounded splice")
                .expect("bounded root");
                let observed = atomic_splice_work_units(before, receipt);
                maximum_observed_work = maximum_observed_work.max(observed);
                assert!(
                    observed <= MAX_SEQUENCE_ATOMIC_SPLICE_WORK_UNITS,
                    "atomic splice used {observed} structural work units at leaf {start}"
                );
                assert!(receipt.maximum_atomic_height <= MAX_SEQUENCE_AVL_HEIGHT);
                session.suspend().expect("suspend bounded splice")
            };
            arena.abort_build(build).expect("abort bounded splice");
            drain(&mut arena);
            assert_eq!(arena.metrics().resident_nodes, resident_base_nodes);
        }
        assert!(maximum_observed_work > u64::from(MAX_SEQUENCE_AVL_HEIGHT));

        assert!(base.release(&mut arena).is_ok());
        drain(&mut arena);
        assert_eq!(arena.metrics().resident_nodes, 0);
    }

    #[test]
    fn cancellation_mid_carry_reclaims_every_journalled_node() {
        let mut arena = PageArena::new(limits(32)).expect("arena");
        let mut receipt = SequenceMutationReceipt::default();
        let mut session = arena.begin_build().expect("build");
        let mut builder =
            ResumableMeasuredSequenceBuilder::<TestSpec>::try_new(&mut session, &mut receipt)
                .expect("builder");
        for value in [1_u64, 2] {
            let leaf = session.allocate(&leaf_payload(value), &[]).expect("leaf");
            builder
                .begin_push(&session, leaf, &mut receipt)
                .expect("begin push");
            let progress = builder
                .poll_push(&mut session, &mut receipt)
                .expect("poll push");
            if value == 1 {
                assert_eq!(progress, ResumableSequenceProgress::Complete);
            } else {
                assert_eq!(progress, ResumableSequenceProgress::Pending);
            }
        }
        let build = session.suspend().expect("suspend cancellation");
        drop(builder);
        arena.abort_build(build).expect("abort");
        drain(&mut arena);
        assert_eq!(arena.metrics().resident_nodes, 0);
        assert_eq!(arena.metrics().live_builds, 0);
    }

    #[test]
    fn terminal_poll_error_poisons_builder_and_abort_reclaims_mutation() {
        let mut arena = PageArena::new(limits(256)).expect("arena");
        {
            let mut receipt = SequenceMutationReceipt::default();
            let mut session = arena.begin_build().expect("build");
            let mut builder =
                ResumableMeasuredSequenceBuilder::<TestSpec>::try_new(&mut session, &mut receipt)
                    .expect("builder");

            let first = session.allocate(&leaf_payload(1), &[]).expect("first");
            builder
                .begin_push(&session, first, &mut receipt)
                .expect("begin first");
            assert_eq!(
                builder
                    .poll_push(&mut session, &mut receipt)
                    .expect("poll first"),
                ResumableSequenceProgress::Complete
            );

            let second = session.allocate(&leaf_payload(2), &[]).expect("second");
            builder
                .begin_push(&session, second, &mut receipt)
                .expect("begin second");
            // Force a receipt overflow after make_branch has allocated the
            // parent and transferred both child owners. Retrying this builder
            // must not interpret its now-consumed local state as completion.
            receipt.branches_allocated = usize::MAX;
            assert_eq!(
                builder.poll_push(&mut session, &mut receipt),
                Err(TestError::Invalid("sequence branch count overflow"))
            );
            assert_eq!(
                builder.poll_push(&mut session, &mut receipt),
                Err(TestError::Invalid("sequence builder is poisoned"))
            );
            assert_eq!(
                builder.begin_finish(&session, &mut receipt),
                Err(TestError::Invalid("sequence builder is poisoned"))
            );
            // Dropping the active session schedules the journalled parent for
            // fuelled abort; no bespoke partial-state rollback is required.
        }
        drain(&mut arena);
        assert_eq!(arena.metrics().resident_nodes, 0);
        assert_eq!(arena.metrics().live_builds, 0);
    }

    #[test]
    fn seal_errors_preserve_the_capability_and_cancel_fuelfully() {
        let mut arena = PageArena::new(limits(64)).expect("arena");
        let mut foreign = PageArena::new(limits(64)).expect("foreign arena");
        let mut seal = armed_multi_owner_seal(&mut arena);

        assert!(matches!(
            seal.poll(&mut foreign, 1),
            Err(ArenaError::StaleBuild)
        ));
        let poll = seal.poll(&mut arena, 1).expect("partial seal poll");
        assert_eq!(poll.transitions, 1);
        assert_eq!(poll.remaining_non_root_owners, 2);
        assert!(poll.root.is_none());

        let failure = seal
            .abort(&mut foreign)
            .expect_err("foreign abort must return the armed seal");
        assert_eq!(failure.error, ArenaError::StaleBuild);
        assert!(failure.seal.abort(&mut arena).is_ok());

        while arena.metrics().pending_build_aborts != 0 || arena.metrics().pending_reclaims != 0 {
            let receipt = arena.poll_reclaim(1);
            assert!(receipt.transitions <= 1);
        }
        assert_eq!(arena.metrics().resident_nodes, 0);
        assert_eq!(arena.metrics().live_builds, 0);
    }

    #[test]
    fn committed_release_error_returns_the_typed_root() {
        let mut arena = PageArena::new(limits(64)).expect("arena");
        let mut foreign = PageArena::new(limits(64)).expect("foreign arena");
        let root = four_leaf_tree(&mut arena);
        let failure = root
            .release(&mut foreign)
            .expect_err("foreign release must return the typed root");
        assert_eq!(failure.error, ArenaError::ForeignArena);
        assert!(failure.root.release(&mut arena).is_ok());
        drain(&mut arena);
        assert_eq!(arena.metrics().resident_nodes, 0);
    }

    #[test]
    fn armed_seal_and_committed_root_drop_guards_are_active() {
        let seal_guard = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut arena = PageArena::new(limits(16)).expect("seal arena");
            let seal = armed_multi_owner_seal(&mut arena);
            drop(seal);
        }));
        assert!(seal_guard.is_err());

        let root_guard = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut arena = PageArena::new(limits(64)).expect("root arena");
            let root = four_leaf_tree(&mut arena);
            drop(root);
        }));
        assert!(root_guard.is_err());
    }

    #[test]
    fn wrong_session_is_rejected_before_builder_state_changes() {
        let mut arena = PageArena::new(limits(16)).expect("arena");
        let mut receipt = SequenceMutationReceipt::default();
        let (first_build, leaf, mut builder) = {
            let mut first = arena.begin_build().expect("first build");
            let builder =
                ResumableMeasuredSequenceBuilder::<TestSpec>::try_new(&mut first, &mut receipt)
                    .expect("builder");
            let leaf = first.allocate(&leaf_payload(1), &[]).expect("leaf");
            let build = first.suspend().expect("suspend first");
            (build, leaf, builder)
        };
        {
            let second = arena.begin_build().expect("second build");
            assert_eq!(
                builder.begin_push(&second, leaf, &mut receipt),
                Err(TestError::Arena(ArenaError::StaleBuild))
            );
        }
        assert_eq!(receipt.leaves_adopted, 0);
        arena.abort_build(first_build).expect("abort first");
        drop(builder);
        drain(&mut arena);
        assert_eq!(arena.metrics().resident_nodes, 0);
    }

    #[test]
    fn malformed_and_forged_nodes_fail_closed() {
        let mut arena = PageArena::new(limits(32)).expect("arena");
        let (build, root) = {
            let mut session = arena.begin_build().expect("build");
            let one = session.allocate(&leaf_payload(1), &[]).expect("one");
            let two = session.allocate(&leaf_payload(2), &[]).expect("two");
            let root = session
                .allocate(&branch_payload(2, 2, 99), &[one.id(), two.id()])
                .expect("forged branch");
            session.release(one).expect("release one");
            session.release(two).expect("release two");
            let build = session.suspend().expect("suspend");
            (build, root)
        };
        let raw_root = root.id();
        let root = seal(
            &mut arena,
            build,
            MeasuredSequenceBuildRoot {
                owner: root,
                marker: PhantomData::<TestSpec>,
            },
        );
        let raw = MeasuredSequenceRef::<TestSpec>::from_raw_root(Some(raw_root));
        assert_eq!(
            raw.summary(&arena, &mut SequenceInspectionReceipt::default()),
            Err(TestError::Invalid(
                "sequence branch measure does not match its children"
            ))
        );
        assert!(root.release(&mut arena).is_ok());
        drain(&mut arena);

        let (build, root) = {
            let mut session = arena.begin_build().expect("child count build");
            let leaf = session.allocate(&leaf_payload(1), &[]).expect("leaf");
            let root = session
                .allocate(&branch_payload(2, 2, 2), &[leaf.id()])
                .expect("bad child count");
            session.release(leaf).expect("release leaf");
            let build = session.suspend().expect("suspend");
            (build, root)
        };
        let raw_root = root.id();
        let root = seal(
            &mut arena,
            build,
            MeasuredSequenceBuildRoot {
                owner: root,
                marker: PhantomData::<TestSpec>,
            },
        );
        let raw = MeasuredSequenceRef::<TestSpec>::from_raw_root(Some(raw_root));
        assert_eq!(
            raw.summary(&arena, &mut SequenceInspectionReceipt::default()),
            Err(TestError::Invalid(
                "sequence branch has the wrong child count"
            ))
        );
        assert!(root.release(&mut arena).is_ok());
        drain(&mut arena);

        let (build, root) = {
            let mut session = arena.begin_build().expect("height build");
            let one = session.allocate(&leaf_payload(1), &[]).expect("one");
            let two = session.allocate(&leaf_payload(2), &[]).expect("two");
            let root = session
                .allocate(&branch_payload(2, 127, 3), &[one.id(), two.id()])
                .expect("impossible height");
            session.release(one).expect("release one");
            session.release(two).expect("release two");
            let build = session.suspend().expect("suspend");
            (build, root)
        };
        let raw_root = root.id();
        let root = seal(
            &mut arena,
            build,
            MeasuredSequenceBuildRoot {
                owner: root,
                marker: PhantomData::<TestSpec>,
            },
        );
        let raw = MeasuredSequenceRef::<TestSpec>::from_raw_root(Some(raw_root));
        assert_eq!(
            raw.summary(&arena, &mut SequenceInspectionReceipt::default()),
            Err(TestError::Invalid(
                "sequence branch height is impossible for its leaf count"
            ))
        );
        assert!(root.release(&mut arena).is_ok());
        drain(&mut arena);
    }

    #[test]
    fn foreign_and_retired_raw_roots_fail_closed() {
        let mut arena = PageArena::new(limits(64)).expect("arena");
        let mut foreign = PageArena::new(limits(64)).expect("foreign arena");
        let foreign_root = four_leaf_tree(&mut foreign);
        let foreign_id = foreign_root.as_ref().root.expect("root id");
        let foreign_ref = MeasuredSequenceRef::<TestSpec>::from_raw_root(Some(foreign_id));
        assert_eq!(
            foreign_ref.summary(&arena, &mut SequenceInspectionReceipt::default()),
            Err(TestError::Arena(ArenaError::ForeignArena))
        );

        let root = four_leaf_tree(&mut arena);
        let root_id = root.as_ref().root.expect("root id");
        assert!(root.release(&mut arena).is_ok());
        drain(&mut arena);
        let stale = MeasuredSequenceRef::<TestSpec>::from_raw_root(Some(root_id));
        assert_eq!(
            stale.summary(&arena, &mut SequenceInspectionReceipt::default()),
            Err(TestError::Arena(ArenaError::StaleHandle))
        );

        assert!(foreign_root.release(&mut foreign).is_ok());
        drain(&mut foreign);
    }
}
