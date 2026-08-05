use std::ops::Range;

use comrak::inline_fragment::{InlineFact, InlineFragment};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OriginRunKind {
    /// Logical and physical bytes correspond one-for-one.
    Identity,
    /// The complete logical run corresponds to the complete physical run, but
    /// interior byte boundaries are not independently mappable (CRLF
    /// normalization and replacement decoding).
    Atomic,
    /// Parser-logical content with no physical source bytes.
    Virtual,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LogicalOriginRun {
    pub logical: Range<u32>,
    pub physical: Range<u64>,
    pub kind: OriginRunKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LogicalOriginMap {
    pub leaf_id: u64,
    pub revision: u64,
    pub logical_len: u32,
    pub runs: Vec<LogicalOriginRun>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OriginMapError {
    EmptyRun {
        index: usize,
    },
    LogicalCoverage {
        index: usize,
        expected: u32,
        actual: u32,
    },
    IdentityLengthMismatch {
        index: usize,
    },
    VirtualHasPhysicalBytes {
        index: usize,
    },
    PhysicalOrder {
        index: usize,
    },
    IncompleteLogicalCoverage {
        expected: u32,
        actual: u32,
    },
    FactOutsideLeaf {
        start: u32,
        end: u32,
        leaf_len: u32,
    },
    PartialAtomicMapping {
        run: usize,
        fact: Range<u32>,
    },
    LeafMismatch,
    RevisionMismatch,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MappedInlineFact {
    pub fact: InlineFact,
    /// Zero, one, or many exact physical source runs. Gaps between parts are
    /// stripped block prefixes and are never claimed by the inline fact.
    pub physical_parts: Vec<Range<u64>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComposedInlineFragment {
    pub semantic_facts: Vec<MappedInlineFact>,
    pub projection_facts: Vec<MappedInlineFact>,
}

impl LogicalOriginMap {
    pub fn validate(&self) -> Result<(), OriginMapError> {
        let mut logical_cursor = 0;
        let mut physical_cursor = 0;
        for (index, run) in self.runs.iter().enumerate() {
            if run.logical.start >= run.logical.end {
                return Err(OriginMapError::EmptyRun { index });
            }
            if run.logical.start != logical_cursor {
                return Err(OriginMapError::LogicalCoverage {
                    index,
                    expected: logical_cursor,
                    actual: run.logical.start,
                });
            }
            if run.physical.start < physical_cursor {
                return Err(OriginMapError::PhysicalOrder { index });
            }
            match run.kind {
                OriginRunKind::Identity
                    if u64::from(run.logical.end - run.logical.start)
                        != run.physical.end - run.physical.start =>
                {
                    return Err(OriginMapError::IdentityLengthMismatch { index });
                }
                OriginRunKind::Virtual if run.physical.start != run.physical.end => {
                    return Err(OriginMapError::VirtualHasPhysicalBytes { index });
                }
                _ => {}
            }
            logical_cursor = run.logical.end;
            physical_cursor = run.physical.end;
        }
        if logical_cursor != self.logical_len {
            return Err(OriginMapError::IncompleteLogicalCoverage {
                expected: self.logical_len,
                actual: logical_cursor,
            });
        }
        Ok(())
    }

    pub fn map_fact(&self, fact: InlineFact) -> Result<MappedInlineFact, OriginMapError> {
        let start = fact.logical_start;
        let end = start
            .checked_add(fact.logical_len)
            .ok_or(OriginMapError::FactOutsideLeaf {
                start,
                end: u32::MAX,
                leaf_len: self.logical_len,
            })?;
        if start > end || end > self.logical_len {
            return Err(OriginMapError::FactOutsideLeaf {
                start,
                end,
                leaf_len: self.logical_len,
            });
        }
        let mut physical_parts = Vec::new();
        let first = self.runs.partition_point(|run| run.logical.end <= start);
        for (index, run) in self.runs.iter().enumerate().skip(first) {
            if run.logical.start >= end {
                break;
            }
            let intersection = start.max(run.logical.start)..end.min(run.logical.end);
            if intersection.start >= intersection.end {
                continue;
            }
            match run.kind {
                OriginRunKind::Identity => {
                    let offset = u64::from(intersection.start - run.logical.start);
                    physical_parts.push(
                        run.physical.start + offset
                            ..run.physical.start
                                + offset
                                + u64::from(intersection.end - intersection.start),
                    );
                }
                OriginRunKind::Atomic => {
                    if intersection != run.logical {
                        return Err(OriginMapError::PartialAtomicMapping {
                            run: index,
                            fact: start..end,
                        });
                    }
                    physical_parts.push(run.physical.clone());
                }
                OriginRunKind::Virtual => {
                    if intersection != run.logical {
                        return Err(OriginMapError::PartialAtomicMapping {
                            run: index,
                            fact: start..end,
                        });
                    }
                }
            }
        }
        Ok(MappedInlineFact {
            fact,
            physical_parts,
        })
    }

    pub fn compose(
        &self,
        fragment: &InlineFragment,
    ) -> Result<ComposedInlineFragment, OriginMapError> {
        self.validate()?;
        if self.leaf_id != fragment.leaf_id {
            return Err(OriginMapError::LeafMismatch);
        }
        if self.revision != fragment.revision {
            return Err(OriginMapError::RevisionMismatch);
        }
        Ok(ComposedInlineFragment {
            semantic_facts: fragment
                .facts
                .iter()
                .copied()
                .map(|fact| self.map_fact(fact))
                .collect::<Result<_, _>>()?,
            projection_facts: fragment
                .projection_facts
                .iter()
                .copied()
                .map(|fact| self.map_fact(fact))
                .collect::<Result<_, _>>()?,
        })
    }
}
